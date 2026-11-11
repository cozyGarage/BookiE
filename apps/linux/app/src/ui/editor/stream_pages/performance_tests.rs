use super::super::pinned::PinnedResults;
use super::super::{StatementIdentity, StatementOutcome, StatementOutcomeKind};
use super::tests::page;
use super::*;
use bookie_core::Value;

fn byte_page(bytes: usize) -> Arc<QueryResult> {
    let mut result = page(&["payload"], &[]).as_ref().clone();
    result.columns[0].data_type = "BYTEA".into();
    result.rows = vec![vec![Value::Bytes(vec![7; bytes])]].into();
    Arc::new(result)
}

#[test]
fn sustained_small_streams_keep_all_pages_within_the_byte_budget() {
    let mut cache = StreamPageCache::default();
    let mut retired = VecDeque::new();
    for index in 0..20_000 {
        let result = page(&["id"], &[index]);
        retired.push_back(Arc::downgrade(&result));
        cache.append(index as u64, result).unwrap();
    }
    let snapshot = cache.snapshot(StreamTerminal::Complete, None);
    assert_eq!(snapshot.total_rows, 20_000);
    assert_eq!(snapshot.page_indices, (0..20_000).collect::<Vec<_>>());
    assert!(retired.iter().all(|page| page.upgrade().is_some()));
    drop(snapshot);
    drop(cache);
    assert!(retired.iter().all(|result| result.upgrade().is_none()));
}

#[test]
fn snapshots_share_payloads_and_release_evicted_pages_after_the_last_owner() {
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    let mut cache = StreamPageCache::default();
    let mut wide_page = page(&["payload"], &[]).as_ref().clone();
    wide_page.rows = vec![vec![Value::Text("x".repeat(24 * 1024 * 1024))]].into();
    let result = Arc::new(wide_page);
    let weak = Arc::downgrade(&result);
    cache.append(0, result).unwrap();
    let first = cache.snapshot_with_spool(StreamTerminal::Loading, None, 0, Some(spool.clone()));
    let second = cache.snapshot_with_spool(StreamTerminal::Loading, None, 0, Some(spool));
    assert!(Weak::ptr_eq(&first.pages[0], &second.pages[0]));
    assert!(Arc::ptr_eq(&first.pages[0].upgrade().unwrap(), &cache.pages()[0].1));
    for index in 1..4 {
        let mut next_page = page(&["payload"], &[]).as_ref().clone();
        next_page.rows = vec![vec![Value::Text("x".repeat(24 * 1024 * 1024))]].into();
        cache.append(index, Arc::new(next_page)).unwrap();
    }
    assert_eq!(cache.first_page(), 2);
    assert!(weak.upgrade().is_none());
    assert!(first.pages[0].upgrade().is_none());
    drop(first);
    drop(second);
    assert!(weak.upgrade().is_none());
}

#[test]
fn independent_pinned_streams_release_pages_and_keep_spools_until_unpin() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let manager = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default();
    let mut displayed = Vec::new();
    let mut pins = Vec::new();
    let mut pages = Vec::new();

    for _ in 0..5 {
        let mut cache = StreamPageCache::default();
        let spool = manager.create().unwrap();
        for page_index in 0..4 {
            let page = byte_page(64 * 1024);
            pages.push(Arc::downgrade(&page));
            runtime.block_on(spool.append_page(page_index, page.clone())).unwrap();
            cache.append(page_index, page).unwrap();
        }
        let outcome = StatementOutcome {
            identity: StatementIdentity::default(),
            sql_preview: "SELECT payload FROM items".into(),
            sql: "SELECT payload FROM items".into(),
            elapsed_ms: 0,
            kind: StatementOutcomeKind::Streamed(Box::new(cache.snapshot_with_spool(
                StreamTerminal::Complete,
                None,
                0,
                Some(spool.clone()),
            ))),
            pinned: false,
        };
        drop(spool);
        displayed.push(outcome.clone());
        let mut pin = PinnedResults::default();
        pin.add_pin(outcome);
        pins.push(pin);
    }

    assert!(pages.iter().all(|page| page.upgrade().is_none()));
    assert!(manager.reserved_bytes() > 0);
    let Some(StatementOutcomeKind::Streamed(result)) = pins[0].iter().next().map(|outcome| &outcome.kind) else {
        panic!("the pinned result is not streamed");
    };
    let Some(spool) = result.spool.as_ref() else {
        panic!("the completed pinned result has no spool");
    };
    let restored = runtime.block_on(spool.load_page(0, result.columns.clone())).unwrap();
    assert_eq!(restored.rows[0][0], Value::Bytes(vec![7; 64 * 1024]));
    drop(displayed);
    assert!(manager.reserved_bytes() > 0);
    for pin in &mut pins {
        assert_eq!(pin.count(), 1);
        assert!(pin.unpin_all().is_empty());
    }
    drop(pins);
    assert_eq!(manager.reserved_bytes(), 0);
}

#[test]
fn pin_eviction_and_unpin_release_unspooled_result_pages() {
    let mut pins = PinnedResults::default();
    let mut pages = Vec::new();

    for index in 0..=super::super::pinned::MAX_PINNED {
        let mut cache = StreamPageCache::default();
        let result = page(&["id"], &[i64::try_from(index).unwrap()]);
        pages.push(Arc::downgrade(&result));
        cache.append(0, result).unwrap();
        let outcome = StatementOutcome {
            identity: StatementIdentity::default(),
            sql_preview: "SELECT id FROM items".into(),
            sql: "SELECT id FROM items".into(),
            elapsed_ms: 0,
            kind: StatementOutcomeKind::Streamed(Box::new(cache.snapshot(StreamTerminal::Complete, None))),
            pinned: false,
        };
        drop(cache);
        pins.add_pin(outcome);
    }

    assert!(pages[0].upgrade().is_none());
    assert!(pages[1..].iter().all(|page| page.upgrade().is_some()));
    assert!(pins.unpin_all().is_empty());
    assert!(pages.iter().all(|page| page.upgrade().is_none()));
}

#[test]
fn completed_spooled_snapshots_release_resident_pages_and_reload_from_the_spool() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    let mut cache = StreamPageCache::default();
    let mut references = Vec::new();
    let mut columns = Vec::new();
    for index in 0_u64..12 {
        let page = page(&["id"], &[index as i64]);
        if index == 0 {
            columns.clone_from(&page.columns);
        }
        references.push(Arc::downgrade(&page));
        runtime.block_on(spool.append_page(index, page.clone())).unwrap();
        cache.append(index, page).unwrap();
    }

    let snapshot = cache.snapshot_with_spool(StreamTerminal::Complete, None, 0, Some(spool.clone()));

    assert_eq!(snapshot.total_pages, 12);
    assert!(snapshot.page_indices.is_empty());
    assert_eq!(snapshot.retained_rows, 0);
    assert!(references.iter().all(|page| page.upgrade().is_none()));
    assert!(snapshot.resident_pages.get(0).is_none());
    let restored = runtime.block_on(spool.load_page(0, columns)).unwrap();
    assert_eq!(restored.rows, vec![vec![Value::Int(0)]]);
}

#[test]
fn terminal_snapshot_keeps_the_page_whose_spool_write_failed() {
    let mut cache = StreamPageCache::default();
    let first = page(&["id"], &[1]);
    let first_ref = Arc::downgrade(&first);
    cache.append(0, first).unwrap();
    let last = page(&["id"], &[2]);
    let last_ref = Arc::downgrade(&last);
    cache.append_with_storage_status(1, last, false).unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();

    let snapshot = cache.snapshot_with_spool(StreamTerminal::Failed("spool failed".into()), None, 0, Some(spool));

    assert_eq!(snapshot.total_pages, 2);
    assert_eq!(snapshot.page_indices, vec![1]);
    assert_eq!(snapshot.retained_rows, 1);
    assert!(first_ref.upgrade().is_none());
    assert_eq!(last_ref.upgrade().unwrap().rows, vec![vec![Value::Int(2)]]);
}

#[test]
fn exactly_sixty_four_mib_is_retained_and_the_next_row_evicts_an_old_page() {
    let overhead = byte_page(0).estimated_row_bytes();
    let mut cache = StreamPageCache::default();
    for index in 0..2 {
        cache.append(index, byte_page(32 * 1024 * 1024 - overhead)).unwrap();
    }
    let snapshot = cache.snapshot(StreamTerminal::Loading, None);
    assert_eq!(snapshot.pages.len(), 2);
    assert_eq!(
        snapshot
            .pages
            .iter()
            .filter_map(Weak::upgrade)
            .map(|page| page.estimated_row_bytes())
            .sum::<usize>(),
        64 * 1024 * 1024
    );
    drop(snapshot);
    cache.append(2, byte_page(1)).unwrap();
    assert_eq!(cache.first_page(), 1);
    assert_eq!(cache.pages().len(), 2);
}

#[test]
fn rejected_pages_release_payloads_without_advancing_the_retained_window() {
    let mut cache = StreamPageCache::default();
    cache.append(0, page(&["id"], &[1])).unwrap();
    let mut malformed = page(&["id"], &[2]).as_ref().clone();
    malformed.rows = vec![vec![Value::Int(1), Value::Int(2), Value::Int(3)]].into();
    for (index, result, expected) in [
        (2, page(&["id"], &[2]), PageAppendError::OutOfOrder),
        (1, page(&["changed"], &[2]), PageAppendError::SchemaChanged),
        (1, Arc::new(malformed), PageAppendError::InvalidRowWidth),
    ] {
        let weak = Arc::downgrade(&result);
        assert_eq!(cache.append(index, result), Err(expected));
        assert!(weak.upgrade().is_none());
        assert_eq!(cache.total_rows(), 1);
        assert_eq!(cache.pages().len(), 1);
    }
    cache.append(1, page(&["id"], &[2])).unwrap();
    assert_eq!(cache.total_rows(), 2);
}

#[test]
fn repeated_empty_pages_do_not_accumulate_or_replace_retained_rows() {
    let mut cache = StreamPageCache::default();
    cache.append(0, page(&["id"], &[1])).unwrap();
    for index in 1..=1_000 {
        let result = page(&["id"], &[]);
        let weak = Arc::downgrade(&result);
        cache.append(index, result).unwrap();
        assert!(weak.upgrade().is_none());
        assert_eq!(cache.pages().len(), 1);
    }
    cache.append(1_001, page(&["id"], &[2])).unwrap();
    let snapshot = cache.snapshot(StreamTerminal::Complete, None);
    assert_eq!(snapshot.total_rows, 2);
    assert_eq!(snapshot.total_pages, 2);
    assert_eq!(snapshot.page_indices, vec![0, 1_001]);
}

#[test]
fn terminal_snapshots_keep_only_the_retained_window_and_release_on_drop() {
    let mut cache = StreamPageCache::default();
    let mut refs = Vec::new();
    for index in 0..12 {
        let mut result = page(&["payload"], &[]).as_ref().clone();
        result.rows = vec![vec![Value::Text("x".repeat(15 * 1024 * 1024))]].into();
        let result = Arc::new(result);
        refs.push(Arc::downgrade(&result));
        cache.append(index as u64, result).unwrap();
    }
    let snapshots: Vec<_> = [
        StreamTerminal::Complete,
        StreamTerminal::Cancelled,
        StreamTerminal::TimedOut,
        StreamTerminal::Failed("fixture failure".into()),
    ]
    .into_iter()
    .map(|terminal| cache.snapshot(terminal, None))
    .collect();
    for snapshot in &snapshots {
        assert_eq!(snapshot.page_indices, vec![8, 9, 10, 11]);
        assert_eq!(snapshot.total_rows, 12);
    }
    drop(cache);
    assert!(refs[..8].iter().all(|result| result.upgrade().is_none()));
    assert!(refs[8..].iter().all(|result| result.upgrade().is_some()));
    drop(snapshots);
    assert!(refs.iter().all(|result| result.upgrade().is_none()));
}

#[test]
fn independent_active_streams_release_their_own_pages() {
    let mut caches = Vec::new();
    let mut snapshots = Vec::new();
    let mut pages = Vec::new();
    for _ in 0..3 {
        let mut cache = StreamPageCache::default();
        let result = byte_page(4 * 1024 * 1024);
        pages.push(Arc::downgrade(&result));
        cache.append(0, result).unwrap();
        snapshots.push(cache.snapshot(StreamTerminal::Loading, None));
        caches.push(cache);
    }
    assert!(pages.iter().all(|page| page.upgrade().is_some()));
    drop(caches.remove(0));
    drop(snapshots.remove(0));
    assert!(pages[0].upgrade().is_none());
    assert!(pages[1..].iter().all(|page| page.upgrade().is_some()));
    drop(snapshots);
    drop(caches);
    assert!(pages.iter().all(|page| page.upgrade().is_none()));
}

#[test]
fn completed_stream_export_keeps_spool_after_result_and_pin_are_dropped() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let manager = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default();
    let spool = manager.create().unwrap();
    let mut cache = StreamPageCache::default();
    let result = byte_page(64 * 1024);
    let weak = Arc::downgrade(&result);
    runtime.block_on(spool.append_page(0, result.clone())).unwrap();
    cache.append(0, result).unwrap();
    let snapshot = cache.snapshot_with_spool(
        StreamTerminal::Complete,
        Some(crate::ui::export_dialog::StreamExportQuery {
            driver_id: "postgres".into(),
            suggested_name: "result".into(),
        }),
        0,
        Some(spool.clone()),
    );
    let export = snapshot.export.clone().expect("complete stream can be exported");
    let outcome = StatementOutcome {
        identity: StatementIdentity::default(),
        sql_preview: "SELECT payload".into(),
        sql: "SELECT payload".into(),
        elapsed_ms: 0,
        kind: StatementOutcomeKind::Streamed(Box::new(snapshot)),
        pinned: false,
    };
    let mut pins = PinnedResults::default();
    pins.add_pin(outcome);
    drop(spool);
    drop(cache);
    drop(pins);
    assert!(weak.upgrade().is_none());
    assert!(manager.reserved_bytes() > 0);
    let restored = runtime
        .block_on(export.spool.as_ref().unwrap().load_page(0, export.columns.clone()))
        .unwrap();
    assert_eq!(restored.rows[0][0], Value::Bytes(vec![7; 64 * 1024]));
    drop(export);
    assert_eq!(manager.reserved_bytes(), 0);
}
