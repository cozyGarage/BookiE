use super::super::stream_pages::StreamPageCache;
use super::*;
use bookie_core::{ColumnInfo, QueryResult, Value};
use std::sync::Arc;

fn page(index: u64) -> Arc<QueryResult> {
    let mut rows: Vec<_> = (0..5_000)
        .map(|row| vec![Value::Int(index as i64 * 5_000 + row), Value::Text(String::new())])
        .collect();
    if let Some(payload) = rows.first_mut().and_then(|row| row.get_mut(1)) {
        *payload = Value::Text("x".repeat(1024 * 1024));
    }
    Arc::new(QueryResult {
        columns: ["id", "payload"]
            .into_iter()
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: if name == "id" { "BIGINT" } else { "TEXT" }.into(),
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
        rows: rows.into(),
        truncated: false,
    })
}

fn clear(holder: &gtk::Box) {
    while let Some(child) = holder.first_child() {
        holder.remove(&child);
    }
}

fn append_live_page(
    holder: &gtk::Box,
    outcome: &StatementOutcome,
    target: &super::super::stream_view::LiveStreamTarget<'_>,
    cursor: &mut super::super::stream_view::EvictionCursor,
) {
    let StatementOutcomeKind::Streamed(result) = &outcome.kind else {
        panic!("expected a streamed outcome");
    };
    if holder.first_child().is_none() {
        holder.append(&build_outcome_widget(
            outcome,
            target.grid_sender,
            target.connection_id,
            target.database.clone(),
            target.settings_store.clone(),
            None,
        ));
        return;
    }
    assert!(super::super::stream_view::update_live_stream(
        holder, outcome, result, target, cursor
    ));
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn streamed_widgets_keep_wide_pages_charged_until_close() {
    gtk::init().unwrap();
    let budget = bookie_results::stream_page_budget::StreamPageBudget::default();
    let mut cache = StreamPageCache::with_budget(budget.clone());
    let mut pages = Vec::new();
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let (sender, _receiver) = relm4::channel();
    let settings = LocalResultSettingsStore::default();
    let database = Arc::new(crate::services::database_service::DatabaseService::new_isolated());
    let mut cursor = super::super::stream_view::EvictionCursor::default();
    let mut retained_bytes = 0;
    let make_outcome = |streamed| StatementOutcome {
        identity: StatementIdentity::default(),
        sql_preview: "SELECT id, payload FROM items".into(),
        sql: "SELECT id, payload FROM items".into(),
        elapsed_ms: 0,
        kind: StatementOutcomeKind::Streamed(Box::new(streamed)),
        pinned: false,
        detail: Default::default(),
    };
    for index in 0..12 {
        let result = page(index);
        retained_bytes += result.estimated_row_bytes();
        pages.push(Arc::downgrade(&result));
        cache.append(index, result).unwrap();
        let outcome = make_outcome(cache.snapshot(StreamTerminal::Loading, None));
        append_live_page(
            &holder,
            &outcome,
            &super::super::stream_view::LiveStreamTarget {
                grid_sender: &sender,
                connection_id: None,
                database: database.clone(),
                settings_store: settings.clone(),
            },
            &mut cursor,
        );
        drop(outcome);
        assert!(pages.iter().all(|page| page.upgrade().is_some()));
    }
    clear(&holder);
    let outcome = make_outcome(cache.snapshot(StreamTerminal::Cancelled, None));
    holder.append(&build_outcome_widget(&outcome, &sender, None, database, settings, None));
    drop(outcome);
    drop(cache);
    assert_eq!(budget.used_bytes(), retained_bytes);
    assert_eq!(
        pages.iter().filter(|page| page.upgrade().is_some()).count(),
        pages.len()
    );
    clear(&holder);
    drop(holder);
    assert!(
        pages.iter().all(|page| page.upgrade().is_none()),
        "closing cancelled results must release their pages"
    );
    assert_eq!(budget.used_bytes(), 0);
    assert_eq!(budget.used_pages(), 0);
}
