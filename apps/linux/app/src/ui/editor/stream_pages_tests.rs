use super::*;
use bookie_core::Value;

pub(super) fn page(columns: &[&str], values: &[i64]) -> Arc<QueryResult> {
    Arc::new(QueryResult {
        columns: columns
            .iter()
            .map(|name| ColumnInfo {
                name: (*name).into(),
                data_type: "BIGINT".into(),
                nullable: false,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            })
            .collect(),
        rows: values.iter().map(|value| vec![Value::Int(*value)]).collect(),
        truncated: false,
    })
}

#[test]
fn retains_all_small_pages_within_the_byte_budget_and_counts_all_delivered_rows() {
    let mut cache = StreamPageCache::default();
    for index in 0..20 {
        cache
            .append(index, page(&["id"], &[index as i64]))
            .expect("append page");
    }

    assert_eq!(cache.pages().len(), 20);
    assert_eq!(cache.first_page(), 0);
    assert_eq!(cache.total_rows(), 20);
    assert_eq!(
        cache.pages().iter().map(|(index, _, _, _)| *index).collect::<Vec<_>>(),
        (0..20).collect::<Vec<_>>()
    );
    let snapshot = cache.snapshot(StreamTerminal::Complete, None);
    assert_eq!(snapshot.total_pages, 20);
    assert_eq!(snapshot.page_indices, (0..20).collect::<Vec<_>>());
    assert_eq!(snapshot.page_row_starts, (1..=20).collect::<Vec<_>>());
    assert_eq!(snapshot.retained_rows, 20);
}

#[test]
fn spooled_pages_stay_indexed_when_the_shared_resident_budget_is_full() {
    let mut cache = StreamPageCache::with_budget(bookie_results::stream_page_budget::StreamPageBudget::new(0));
    cache
        .append_with_storage_status(0, page(&["id"], &[1]), true)
        .expect("spooled page remains part of the result");

    let snapshot = cache.snapshot(StreamTerminal::Complete, None);
    assert_eq!(snapshot.total_pages, 1);
    assert_eq!(snapshot.total_rows, 1);
    assert!(snapshot.page_indices.is_empty());
    assert_eq!(snapshot.retained_rows, 0);
}

#[test]
fn two_streams_share_page_capacity_and_release_it_with_the_last_owner() {
    let first_page = page(&["id"], &[1]);
    let budget = bookie_results::stream_page_budget::StreamPageBudget::new(first_page.estimated_row_bytes());
    let mut first = StreamPageCache::with_budget(budget.clone());
    let mut second = StreamPageCache::with_budget(budget.clone());
    first
        .append_with_storage_status(0, first_page, true)
        .expect("first page");
    second
        .append_with_storage_status(0, page(&["id"], &[2]), true)
        .expect("second page stays spooled");

    assert_eq!(budget.used_bytes(), budget.limit_bytes());
    assert_eq!(budget.used_cells(), 1);
    assert_eq!(second.total_rows(), 1);
    assert!(second.pages().is_empty());
    drop(first);
    assert_eq!(budget.used_bytes(), 0);
    assert_eq!(budget.used_cells(), 0);
}

#[test]
fn spools_from_one_manager_share_the_page_budget() {
    let manager = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default();
    let first = manager.create().expect("first spool");
    let second = manager.create().expect("second spool");
    let budget = first.page_budget();
    let reservation = budget.reserve(budget.limit_bytes()).expect("whole budget");

    assert!(second.page_budget().reserve(1).is_none());
    drop(reservation);
    assert!(second.page_budget().reserve(1).is_some());
}

#[test]
fn an_unspooled_page_is_rejected_when_the_shared_budget_cannot_retain_it() {
    let mut cache = StreamPageCache::with_budget(bookie_results::stream_page_budget::StreamPageBudget::new(0));
    let error = cache
        .append_with_storage_status(0, page(&["id"], &[1]), false)
        .expect_err("unspooled data must not be silently discarded");

    assert_eq!(error, PageAppendError::RetentionLimit);
    assert_eq!(cache.total_rows(), 0);
    assert_eq!(cache.first_page(), 0);
    assert!(cache.pages().is_empty());
}

#[test]
fn a_visible_page_keeps_its_budget_lease_after_cache_eviction() {
    let page = page(&["id"], &[1]);
    let budget = bookie_results::stream_page_budget::StreamPageBudget::new(page.estimated_row_bytes());
    let store = ResidentPageStore::with_budget(budget.clone());
    store.insert(0, page.clone(), budget.retain(&page).expect("page lease"), true);
    store.protect(0);
    store.remove(0);

    assert_eq!(budget.used_bytes(), page.estimated_row_bytes());
    assert!(store.get(0).is_some());
    store.unprotect(0);
    assert!(store.get(0).is_none());
}

#[test]
fn restoring_a_cached_page_preserves_cache_ownership_after_the_view_closes() {
    let page = page(&["id"], &[1]);
    let budget = bookie_results::stream_page_budget::StreamPageBudget::new(page.estimated_row_bytes());
    let store = ResidentPageStore::with_budget(budget.clone());
    store.insert(0, page.clone(), budget.retain(&page).expect("cache lease"), true);
    store.protect(0);
    store.insert(0, page.clone(), budget.retain(&page).expect("view lease"), false);
    store.protect(0);
    store.unprotect(0);
    assert!(store.get(0).is_some());
    store.remove(0);
    assert!(store.get(0).is_some());
    store.unprotect(0);
    assert!(store.get(0).is_none());
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn restored_cached_page_remains_available_after_its_last_view_closes() {
    let page = page(&["id"], &[1]);
    let budget = bookie_results::stream_page_budget::StreamPageBudget::new(page.estimated_row_bytes());
    let store = ResidentPageStore::with_budget(budget.clone());
    store.insert(0, page.clone(), budget.retain(&page).unwrap(), true);
    store.insert(0, page.clone(), budget.retain(&page).unwrap(), false);
    store.protect(0);
    store.unprotect(0);
    assert!(store.get(0).is_some());
    store.remove(0);
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn a_second_decode_does_not_replace_a_page_held_by_an_existing_view() {
    let first = page(&["id"], &[1]);
    let second = page(&["id"], &[1]);
    let budget = bookie_results::stream_page_budget::StreamPageBudget::new(first.estimated_row_bytes() * 2);
    let store = ResidentPageStore::with_budget(budget.clone());
    store.insert(0, first.clone(), budget.retain(&first).unwrap(), false);
    store.protect(0);
    store.insert(0, second.clone(), budget.retain(&second).unwrap(), false);
    assert!(Arc::ptr_eq(&store.get(0).unwrap(), &first));
    assert_eq!(budget.used_bytes(), first.estimated_row_bytes());
    store.unprotect(0);
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn a_stream_with_an_unspooled_page_cannot_export_as_complete() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::with_limit(1)
        .create()
        .unwrap();
    let page = page(&["id"], &[1]);
    assert!(runtime.block_on(spool.append_page(0, page.clone())).is_err());

    let mut cache = StreamPageCache::default();
    cache.append_with_storage_status(0, page, false).unwrap();
    let snapshot = cache.snapshot_with_spool(
        StreamTerminal::Failed("temporary storage quota reached".into()),
        Some(crate::ui::export_dialog::StreamExportQuery {
            driver_id: "postgres".into(),
            suggested_name: "items".into(),
        }),
        0,
        Some(spool),
    );

    assert_eq!(snapshot.retained_rows, 1);
    assert!(snapshot.export.is_none());
    assert!(matches!(snapshot.terminal, StreamTerminal::Failed(_)));
}

#[test]
fn page_row_starts_keep_short_and_empty_page_offsets() {
    assert_eq!(page_row_starts_from_counts(&[2, 0, 1, 3]), vec![1, 3, 3, 4]);
}

#[test]
fn cache_keeps_row_offsets_across_an_empty_page() {
    let mut cache = StreamPageCache::default();
    cache.append(0, page(&["id"], &[1, 2])).unwrap();
    cache.append(1, page(&["id"], &[])).unwrap();
    cache.append(2, page(&["id"], &[3])).unwrap();

    let snapshot = cache.snapshot(StreamTerminal::Complete, None);

    assert_eq!(snapshot.page_row_counts, vec![2, 0, 1]);
    assert_eq!(snapshot.page_row_starts, vec![1, 3, 3]);
    assert_eq!(snapshot.total_rows, 3);
    assert_eq!(snapshot.retained_rows, 3);
}

#[test]
fn page_row_starts_saturate_when_cumulative_rows_reach_the_type_limit() {
    assert_eq!(page_row_starts_from_counts(&[usize::MAX, 1]), vec![1, usize::MAX]);
}

#[test]
fn wide_pages_evict_by_bytes_before_the_page_count_limit() {
    let mut cache = StreamPageCache::default();
    let mut retired = None;
    for index in 0..3 {
        let mut result = page(&["payload"], &[]).as_ref().clone();
        result.rows = vec![vec![Value::Text("x".repeat(24 * 1024 * 1024))]].into();
        let result = Arc::new(result);
        if index == 0 {
            retired = Some(Arc::downgrade(&result));
        }
        cache.append(index, result).unwrap();
    }
    assert_eq!(cache.pages().len(), 2);
    assert!(retired.unwrap().upgrade().is_none());
    let snapshot = cache.snapshot(StreamTerminal::Complete, None);
    assert_eq!(snapshot.page_indices, vec![1, 2]);
    assert_eq!(snapshot.total_rows, 3);
    assert_eq!(snapshot.total_pages, 3);
    assert_eq!(snapshot.page_row_counts, vec![1, 1, 1]);
    assert_eq!(snapshot.retained_rows, 2);
    assert_eq!(
        snapshot.pages[0].upgrade().unwrap().rows[0][0],
        Value::Text("x".repeat(24 * 1024 * 1024))
    );
}

#[test]
fn the_newest_page_is_kept_even_when_it_alone_exceeds_the_byte_budget() {
    let mut cache = StreamPageCache::default();
    let mut result = page(&["payload"], &[]).as_ref().clone();
    result.rows = vec![vec![Value::Bytes(vec![7; 65 * 1024 * 1024])]].into();
    let result = Arc::new(result);
    let retired = Arc::downgrade(&result);
    cache.append(0, result).unwrap();
    assert_eq!(cache.pages().len(), 1);
    assert_eq!(cache.total_rows(), 1);
    assert_eq!(cache.snapshot(StreamTerminal::Complete, None).retained_rows, 1);
    cache.append(1, page(&["payload"], &[2])).unwrap();
    assert!(retired.upgrade().is_none());
    assert_eq!(cache.first_page(), 1);
    assert_eq!(cache.total_rows(), 2);
    assert_eq!(cache.snapshot(StreamTerminal::Complete, None).retained_rows, 1);
}

#[test]
fn rejects_out_of_order_pages_without_advancing_the_cache() {
    let mut cache = StreamPageCache::default();
    cache.append(0, page(&["id"], &[1])).expect("first page");

    let error = cache.append(2, page(&["id"], &[3])).unwrap_err();

    assert_eq!(error, PageAppendError::OutOfOrder);
    assert_eq!(cache.total_rows(), 1);
    assert_eq!(cache.pages().len(), 1);
}

#[test]
fn rejects_a_late_schema_change_and_keeps_decoded_rows_from_a_truncated_page() {
    let mut cache = StreamPageCache::default();
    cache.append(0, page(&["id"], &[1])).expect("first page");
    let error = cache.append(1, page(&["name"], &[2])).unwrap_err();
    assert_eq!(error, PageAppendError::SchemaChanged);

    let mut truncated = page(&["id"], &[2]).as_ref().clone();
    truncated.truncated = true;
    cache.append(1, Arc::new(truncated)).expect("partial page");
    let result = cache.snapshot(StreamTerminal::Failed("page budget exceeded".into()), None);
    assert_eq!(result.total_rows, 2);
    assert!(result.pages[1].upgrade().unwrap().truncated);
    assert!(matches!(result.terminal, StreamTerminal::Failed(_)));
}

#[test]
fn an_empty_final_page_keeps_result_metadata() {
    let mut cache = StreamPageCache::default();
    let empty = Arc::new(QueryResult {
        columns: page(&["id"], &[]).columns.clone(),
        rows: Vec::new().into(),
        truncated: false,
    });
    cache.append(0, empty).expect("empty result page");

    assert_eq!(cache.total_rows(), 0);
    assert_eq!(cache.columns().map(|columns| columns[0].name.as_str()), Some("id"));
}

#[test]
fn an_empty_page_after_the_first_ends_the_stream_without_evicting_rows() {
    let mut cache = StreamPageCache::default();
    for index in 0..20 {
        cache.append(index, page(&["id"], &[index as i64])).unwrap();
    }

    cache.append(20, page(&["id"], &[])).unwrap();

    assert_eq!(cache.pages().len(), 20);
    assert_eq!(cache.first_page(), 0);
    let snapshot = cache.snapshot(StreamTerminal::Complete, None);
    assert_eq!(snapshot.total_pages, 20);
    assert!(
        snapshot
            .pages
            .iter()
            .filter_map(Weak::upgrade)
            .all(|page| !page.rows.is_empty())
    );
}

#[test]
fn an_empty_first_page_is_kept_for_its_columns() {
    let mut cache = StreamPageCache::default();
    cache.append(0, page(&["id"], &[])).unwrap();
    assert_eq!(cache.pages().len(), 1);
    assert_eq!(cache.snapshot(StreamTerminal::Complete, None).total_pages, 1);
}

#[test]
fn a_page_after_a_skipped_empty_page_is_still_in_order() {
    let mut cache = StreamPageCache::default();
    cache.append(0, page(&["id"], &[1])).unwrap();
    cache.append(1, page(&["id"], &[])).unwrap();

    cache
        .append(2, page(&["id"], &[2]))
        .expect("the next page is not out of order");

    assert_eq!(cache.snapshot(StreamTerminal::Complete, None).total_pages, 2);
}
