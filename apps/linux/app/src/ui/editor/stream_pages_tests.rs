use super::*;
use tablepro_core::Value;

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
        cache.pages().iter().map(|(index, _, _)| *index).collect::<Vec<_>>(),
        (0..20).collect::<Vec<_>>()
    );
    let snapshot = cache.snapshot(StreamTerminal::Complete, None);
    assert_eq!(snapshot.total_pages, 20);
    assert_eq!(snapshot.page_indices, (0..20).collect::<Vec<_>>());
}

#[test]
fn wide_pages_evict_by_bytes_before_the_page_count_limit() {
    let mut cache = StreamPageCache::default();
    let mut retired = None;
    for index in 0..3 {
        let mut result = page(&["payload"], &[]).as_ref().clone();
        result.rows = vec![vec![Value::Text("x".repeat(24 * 1024 * 1024))]];
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
    assert_eq!(snapshot.pages[0].rows[0][0], Value::Text("x".repeat(24 * 1024 * 1024)));
}

#[test]
fn the_newest_page_is_kept_even_when_it_alone_exceeds_the_byte_budget() {
    let mut cache = StreamPageCache::default();
    let mut result = page(&["payload"], &[]).as_ref().clone();
    result.rows = vec![vec![Value::Bytes(vec![7; 65 * 1024 * 1024])]];
    let result = Arc::new(result);
    let retired = Arc::downgrade(&result);
    cache.append(0, result).unwrap();
    assert_eq!(cache.pages().len(), 1);
    assert_eq!(cache.total_rows(), 1);
    cache.append(1, page(&["payload"], &[2])).unwrap();
    assert!(retired.upgrade().is_none());
    assert_eq!(cache.first_page(), 1);
    assert_eq!(cache.total_rows(), 2);
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
    assert!(result.pages[1].truncated);
    assert!(matches!(result.terminal, StreamTerminal::Failed(_)));
}

#[test]
fn an_empty_final_page_keeps_result_metadata() {
    let mut cache = StreamPageCache::default();
    let empty = Arc::new(QueryResult {
        columns: page(&["id"], &[]).columns.clone(),
        rows: Vec::new(),
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
    assert!(snapshot.pages.iter().all(|page| !page.rows.is_empty()));
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
