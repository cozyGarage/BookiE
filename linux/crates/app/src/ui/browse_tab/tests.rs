use super::grid_render::column_layout_matches;
use super::{BrowsePageRequest, PageRequestTracker, RowCountRequestTracker, columns_for_browse_page};
use tablepro_core::{ColumnInfo, QueryResult, Value};
use uuid::Uuid;

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
    }
}

#[test]
fn only_the_latest_browse_page_request_is_accepted() {
    let tracker = PageRequestTracker::default();
    let older = tracker.begin(0);
    let newer = tracker.begin(0);

    assert!(!tracker.accepts(older, 0));
    assert!(tracker.accepts(newer, 0));
}

#[test]
fn only_the_latest_row_count_request_is_accepted() {
    let tracker = RowCountRequestTracker::default();
    let older = tracker.begin();
    let newer = tracker.begin();

    assert!(!tracker.accepts(older));
    assert!(tracker.accepts(newer));
}

#[test]
fn browse_page_response_must_match_the_current_offset() {
    let tracker = PageRequestTracker::default();
    let request = tracker.begin(100);
    let same_id_wrong_offset = BrowsePageRequest {
        id: request.id,
        offset: 100,
    };

    assert!(!tracker.accepts(same_id_wrong_offset, 200));
    assert_ne!(request.id, Uuid::nil());
}

#[test]
fn mongodb_page_schema_updates_late_fields_and_mixed_types_before_grid_editing() {
    let loaded = vec![column("_id", "ObjectId", true), column("value", "string", false)];
    let page_columns = vec![
        column("_id", "ObjectId", true),
        column("value", "mixed", false),
        column("late_field", "Decimal128", false),
    ];
    let page = QueryResult {
        columns: page_columns.clone(),
        rows: vec![vec![
            Value::Uuid(Uuid::new_v4()),
            Value::Json(serde_json::json!({"$numberDecimal": "1.25"})),
            Value::Text("late".into()),
        ]],
        truncated: false,
    };

    let effective = columns_for_browse_page("mongodb", &loaded, Some(&page));
    assert_eq!(effective, page_columns);
    assert_eq!(effective[1].data_type, "mixed");
    assert_eq!(effective[2].name, "late_field");
    assert!(
        !column_layout_matches(&loaded, &effective),
        "new page metadata must rebuild cached factories and editability"
    );
    let text_value = Value::Text("ordinary text on the mixed page".into());
    assert!(crate::ui::grid::cell_allows_inline_edit(&loaded[1], &text_value));
    assert!(
        !crate::ui::grid::cell_allows_inline_edit(&effective[1], &text_value),
        "page-discovered mixed BSON types must make the same text cell read-only"
    );
}

#[test]
fn page_metadata_does_not_replace_schema_for_other_drivers() {
    let loaded = vec![column("value", "numeric", false)];
    let page = QueryResult {
        columns: vec![column("value", "text", false)],
        rows: Vec::new(),
        truncated: false,
    };
    assert_eq!(columns_for_browse_page("postgres", &loaded, Some(&page)), loaded);
}

#[test]
fn same_count_type_change_invalidates_cached_grid_factories() {
    let rendered = vec![column("value", "string", false)];
    let current = vec![column("value", "mixed", false)];
    assert!(!column_layout_matches(&rendered, &current));
    assert!(column_layout_matches(&current, &current));
}
