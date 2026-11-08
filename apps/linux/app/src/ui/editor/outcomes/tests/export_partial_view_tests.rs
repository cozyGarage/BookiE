use super::*;

#[test]
#[ignore = "requires an isolated GTK display"]
fn an_incomplete_stream_does_not_offer_full_result_export() {
    relm4::adw::init().unwrap();
    let (_runtime, database, origin) = stream_export_origin();
    let page = std::sync::Arc::new(cursor_page(&[1, 2], true).result);
    let request = crate::ui::export_dialog::StreamExportRequest {
        query: crate::ui::export_dialog::StreamExportQuery {
            driver_id: "sqlite".into(),
            suggested_name: "items".into(),
        },
        columns: page.columns.clone(),
        spool: None,
        total_rows: 4,
    };
    let streamed = StreamedResult {
        pages: vec![std::sync::Arc::downgrade(&page)],
        page_indices: vec![2],
        page_row_starts: super::super::super::stream_pages::page_row_starts_from_counts(&[1, 1, page.rows.len(), 1]),
        page_row_counts: vec![1, 1, page.rows.len(), 1],
        retained_rows: page.rows.len(),
        columns: page.columns.clone(),
        total_pages: 4,
        total_rows: bookie_core::MAX_QUERY_CURSOR_PAGE_ROWS * 3 + 2,
        terminal: StreamTerminal::Loading,
        export: Some(request.clone()),
        generation: 0,
        spool: None,
        resident_pages: super::super::super::stream_pages::ResidentPageStore::from_pages([(2, page.clone())]),
    };
    let outcome = super::super::StatementOutcome {
        identity: StatementIdentity {
            statement_index: 0,
            digest: [7; 32],
        },
        sql_preview: "SELECT id FROM items".into(),
        sql: "SELECT id FROM items".into(),
        elapsed_ms: 0,
        kind: StatementOutcomeKind::Streamed(Box::new(streamed.clone())),
        pinned: false,
    };
    let (grid_sender, _receiver) = relm4::channel::<crate::ui::grid::GridMsg>();
    let widget = super::super::build_streamed_result_widget(
        &outcome,
        &streamed,
        &grid_sender,
        Some(origin.id),
        database,
        Default::default(),
        None,
    );

    assert!(stream_export_button(&widget).is_none());
    assert!(contains_label(
        &widget,
        "Export all rows is available when the query completes."
    ));
    assert_eq!(streamed.pages.len(), 1);
    assert!(streamed.total_rows > streamed.pages[0].upgrade().unwrap().rows.len());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_completed_stream_offers_its_saved_result_for_export() {
    relm4::adw::init().unwrap();
    let (runtime, database, origin) = stream_export_origin();
    let page = std::sync::Arc::new(cursor_page(&[1, 2], false).result);
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    runtime.block_on(spool.append_page(0, page.clone())).unwrap();
    let mut cache = super::super::super::stream_pages::StreamPageCache::default();
    cache.append(0, page.clone()).unwrap();
    let streamed = cache.snapshot_with_spool(
        StreamTerminal::Complete,
        Some(crate::ui::export_dialog::StreamExportQuery {
            driver_id: "sqlite".into(),
            suggested_name: "items".into(),
        }),
        0,
        Some(spool),
    );
    let outcome = super::super::StatementOutcome {
        identity: StatementIdentity::default(),
        sql_preview: "SELECT id FROM items".into(),
        sql: "SELECT id FROM items".into(),
        elapsed_ms: 0,
        kind: StatementOutcomeKind::Streamed(Box::new(streamed.clone())),
        pinned: false,
    };
    let (sender, receiver) = relm4::channel::<crate::ui::grid::GridMsg>();
    let widget = super::super::build_streamed_result_widget(
        &outcome,
        &streamed,
        &sender,
        Some(origin.id),
        database,
        Default::default(),
        None,
    );
    stream_export_button(&widget).unwrap().emit_clicked();
    let Some(crate::ui::grid::GridMsg::ExportStreamedResults(request)) = receiver.recv_sync() else {
        panic!("expected a saved result export");
    };
    assert_eq!(request.total_rows, 2);
    assert_eq!(request.spool.unwrap().page_count(), 1);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_filtered_result_exports_only_its_own_saved_rows() {
    relm4::adw::init().unwrap();
    let (runtime, database, origin) = stream_export_origin();
    let page = std::sync::Arc::new(cursor_page(&[1, 2, 3, 4], false).result);
    let manager = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default();
    let spool = manager.create().unwrap();
    runtime.block_on(spool.append_page(0, page.clone())).unwrap();
    let mut cache = super::super::super::stream_pages::StreamPageCache::default();
    cache.append(0, page.clone()).unwrap();
    let source = cache.snapshot_with_spool(
        StreamTerminal::Complete,
        Some(crate::ui::export_dialog::StreamExportQuery {
            driver_id: "sqlite".into(),
            suggested_name: "items".into(),
        }),
        0,
        Some(spool.clone()),
    );
    let scan = bookie_app_services::result_scan::ScanSource::new(spool, source.columns.clone(), 4);
    let filtered = bookie_app_services::result_filter::filter_into_spool(
        &scan,
        bookie_app_services::result_predicate::ResultPredicate::Equals {
            column: 0,
            value: bookie_core::Value::Int(3),
        },
        &manager,
        &tokio_util::sync::CancellationToken::new(),
        &bookie_app_services::result_scan::ScanProgress::default(),
    )
    .unwrap();
    let outcome =
        super::super::super::result_tools::derived_outcome(StatementIdentity::default(), &source, filtered, "3");
    let StatementOutcomeKind::Streamed(derived) = outcome.kind.clone() else {
        panic!("a derived outcome is streamed");
    };
    let (sender, receiver) = relm4::channel::<crate::ui::grid::GridMsg>();
    let widget = super::super::build_streamed_result_widget(
        &outcome,
        &derived,
        &sender,
        Some(origin.id),
        database,
        Default::default(),
        None,
    );
    stream_export_button(&widget).unwrap().emit_clicked();
    let Some(crate::ui::grid::GridMsg::ExportStreamedResults(request)) = receiver.recv_sync() else {
        panic!("expected a saved result export");
    };
    assert_eq!(request.total_rows, 1);
    assert_eq!(request.query.suggested_name, "items-filtered");
    assert_eq!(request.spool.unwrap().page_count(), 1);
    assert!(contains_label(
        &widget,
        "Complete · Received 1 row · 1 page\nEarlier pages can be loaded from temporary storage."
    ));
}
