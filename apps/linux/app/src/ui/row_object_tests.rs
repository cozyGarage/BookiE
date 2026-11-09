use super::*;
use bookie_core::{ColumnInfo, QueryResult};
use gtk4::glib::object::CastNone;
use gtk4::prelude::ListModelExt;
use std::sync::Arc;

#[test]
fn json_preview_writer_stops_after_one_byte_past_its_sample() {
    let mut writer = JsonPreviewWriter {
        prefix: Vec::with_capacity(PREVIEW_BYTES),
        byte_count: 0,
        exceeded: false,
    };
    let chunk = vec![b'x'; PREVIEW_BYTES * 8];
    assert!(std::io::Write::write_all(&mut writer, &chunk).is_err());
    assert_eq!(writer.byte_count, PREVIEW_BYTES + 1);
    assert!(writer.exceeded);
    assert_eq!(writer.prefix, vec![b'x'; PREVIEW_BYTES]);
    assert_eq!(writer.prefix.capacity(), PREVIEW_BYTES);
}

#[test]
fn a_large_parsed_json_preview_keeps_its_sample_and_reports_a_lower_bound() {
    let value = Value::Json(Box::new(serde_json::Value::String("x".repeat(8 * 1024 * 1024))));
    let preview = preview_value(&value).unwrap();
    assert_eq!(preview.byte_count, PreviewByteCount::MoreThan(PREVIEW_BYTES));
    assert_eq!(preview.value, Value::Text(format!("\"{}", "x".repeat(8191))));
    assert!(
        matches!(&value, Value::Json(json) if matches!(json.as_ref(), serde_json::Value::String(text) if text.len() == 8 * 1024 * 1024))
    );
}

#[test]
fn a_json_preview_does_not_replace_the_full_typed_cell_value() {
    let source = Value::Json(Box::new(serde_json::json!({"payload": "x".repeat(100_000)})));
    let expected = source.clone();
    let row = RowObject::new(vec![source]);
    row.preview_values_from_result();

    assert_eq!(
        row.cell_preview(0).unwrap().byte_count,
        PreviewByteCount::MoreThan(PREVIEW_BYTES)
    );
    assert!(row.with_cell_value(0, |value| value == &expected).unwrap());
}

#[test]
fn row_highlight_cache_reuses_rule_identity_and_invalidates_with_row_changes() {
    use bookie_app_services::highlight_match::{HighlightOp, HighlightRule, HighlightTarget, prepare_rules};
    use bookie_core::FilterValue;
    use std::cell::Cell;

    let rules = Arc::from(
        prepare_rules(
            &[HighlightRule {
                column: "status".into(),
                op: HighlightOp::Eq,
                value: Some(FilterValue::Single("bad".into())),
                color: "red".into(),
                target: HighlightTarget::Row,
            }],
            &["status"],
        )
        .unwrap(),
    );
    let row = RowObject::new(vec![Value::Text("bad".into())]);
    let calls = Cell::new(0);
    let initial = row.highlight_row_matches(&rules, || {
        calls.set(calls.get() + 1);
        1
    });
    let reused = row.highlight_row_matches(&rules, || {
        calls.set(calls.get() + 1);
        0
    });
    assert_eq!(initial, 1);
    assert_eq!(reused, 1);
    assert_eq!(calls.get(), 1);

    let replacement_rules = Arc::from(rules.to_vec());
    row.highlight_row_matches(&replacement_rules, || {
        calls.set(calls.get() + 1);
        1
    });
    assert_eq!(calls.get(), 2);

    row.set_cell(0, Value::Text("good".into()));
    let refreshed = row.highlight_row_matches(&rules, || {
        calls.set(calls.get() + 1);
        0
    });
    assert_eq!(refreshed, 0);
    assert_eq!(calls.get(), 3);

    row.preview_values_from_result();
    row.highlight_row_matches(&rules, || {
        calls.set(calls.get() + 1);
        0
    });
    assert_eq!(calls.get(), 4);
}

#[test]
fn parsed_json_preview_matches_serialized_escapes_and_utf8_boundaries() {
    let json = serde_json::json!({"payload": "é😀\0\\\"".repeat(2000)});
    let serialized = json.to_string();
    let preview = preview_value(&Value::Json(Box::new(json))).unwrap();
    let end = serialized.floor_char_boundary(8192);
    assert_eq!(preview.byte_count, PreviewByteCount::MoreThan(PREVIEW_BYTES));
    assert_eq!(preview.value, Value::Text(serialized[..end].to_owned()));
}

#[test]
fn parsed_json_uses_its_serialized_size_at_the_preview_boundary() {
    assert!(preview_value(&Value::Json(Box::new(serde_json::Value::String("x".repeat(8190))))).is_none());
    let preview = preview_value(&Value::Json(Box::new(serde_json::Value::String("x".repeat(8191))))).unwrap();
    assert_eq!(preview.byte_count, PreviewByteCount::MoreThan(PREVIEW_BYTES));
}

#[test]
fn root_json_string_previews_preserve_escaped_and_multibyte_content() {
    for text in ["\\\"".repeat(4095), "\\\"".repeat(4096), "é".repeat(4096)] {
        let json = serde_json::Value::String(text);
        let serialized = json.to_string();
        let preview = preview_value(&Value::Json(Box::new(json)));
        if serialized.len() <= PREVIEW_BYTES {
            assert!(preview.is_none());
            continue;
        }
        let preview = preview.unwrap();
        let end = serialized.floor_char_boundary(PREVIEW_BYTES);
        assert_eq!(preview.byte_count, PreviewByteCount::MoreThan(PREVIEW_BYTES));
        assert_eq!(preview.value, Value::Text(serialized[..end].to_owned()));
    }
}

#[test]
fn json_preview_is_absent_when_the_serialized_value_fits_the_byte_limit() {
    assert!(preview_value(&Value::Json(Box::new(serde_json::json!({"a": 1})))).is_none());
}

#[test]
fn shared_result_rows_do_not_clone_large_cell_payloads_into_row_objects() {
    let payload = "x".repeat(1_000_000);
    let result = Arc::new(QueryResult {
        columns: vec![ColumnInfo {
            name: "payload".into(),
            data_type: "TEXT".into(),
            nullable: false,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }],
        rows: vec![vec![Value::Text(payload.clone())]].into(),
        truncated: false,
    });
    let source_pointer = match &result.rows[0][0] {
        Value::Text(text) => text.as_ptr() as usize,
        _ => 0,
    };
    let store = crate::ui::row_store::RowStore::from_shared_with_result_previews(result.clone());
    let row = store.item(0).and_downcast::<RowObject>().unwrap();

    let row_pointer = row
        .with_cell_value(0, |value| match value {
            Value::Text(text) => text.as_ptr() as usize,
            _ => 0,
        })
        .unwrap();
    assert_eq!(row_pointer, source_pointer);
    assert_eq!(row.cell_value(0), Value::Text(payload.clone()));
    assert_eq!(
        row.cell_preview(0).unwrap().byte_count,
        PreviewByteCount::Exact(payload.len())
    );
}

#[test]
fn row_identity_is_shared_per_key_projection_and_invalidated_by_key_edits() {
    let row = RowObject::new(vec![Value::Text("before".into()), Value::Int(1)]);
    let first = row.row_identity(&[0]);
    let repeated = row.row_identity(&[0]);
    assert!(Arc::ptr_eq(&first, &repeated));
    assert_eq!(first.pk_values.as_ref(), &[Value::Text("before".into())]);
    assert_eq!(
        first.key,
        crate::services::change_tracker::RowKey::from_pk_values(&[Value::Text("before".into())])
    );

    row.set_cell(1, Value::Int(2));
    assert!(Arc::ptr_eq(&first, &row.row_identity(&[0])));

    row.set_cell(0, Value::Text("after".into()));
    let updated = row.row_identity(&[0]);
    assert!(!Arc::ptr_eq(&first, &updated));
    assert_eq!(updated.pk_values.as_ref(), &[Value::Text("after".into())]);
    assert_eq!(
        updated.key,
        crate::services::change_tracker::RowKey::from_pk_values(&[Value::Text("after".into())])
    );

    let other_projection = row.row_identity(&[1]);
    assert_eq!(other_projection.pk_values.as_ref(), &[Value::Int(2)]);
}

#[test]
fn row_identity_keeps_unreadable_key_values_but_refuses_a_persisted_key() {
    let row = RowObject::new(vec![Value::Undecodable("INTERVAL".into())]);
    let identity = row.row_identity(&[0]);
    assert_eq!(identity.pk_values.as_ref(), &[Value::Undecodable("INTERVAL".into())]);
    assert!(identity.key.is_none());
}

#[test]
fn a_server_cut_cell_shows_its_prefix_and_blocks_row_copy_and_export() {
    let partial = bookie_core::partial_value(2_097_152, "begin");
    let row = RowObject::new(vec![Value::Int(7), partial.clone()]);
    row.preview_long_values(&[0]);

    let preview = row.cell_preview(1).expect("the cut cell previews its prefix");
    assert_eq!(preview.value, Value::Text("begin".into()));
    assert_eq!(preview.byte_count, PreviewByteCount::Exact(2_097_152));
    assert!(row.cell_is_partial(1));
    assert!(!row.cell_is_partial(0));
    assert!(!row.cell_is_loaded(1));
    assert!(!row.cells_are_complete());
    assert_eq!(row.complete_cells(), None);
    assert_eq!(row.loaded_cells(), vec![Some(Value::Int(7)), None]);

    row.set_cell(1, Value::Text("the whole value".into()));
    assert!(!row.cell_is_partial(1));
    assert!(row.cell_is_loaded(1));
    assert!(row.cell_preview(1).is_none());
    assert_eq!(
        row.complete_cells(),
        Some(vec![Value::Int(7), Value::Text("the whole value".into())])
    );
}

#[test]
fn projected_cells_distinguish_unfetched_values_from_sql_null() {
    let row = RowObject::new_projected(vec![Value::Null, Value::Text("shown".into())], &[0, 2], 3).unwrap();

    assert!(row.cell_is_loaded(0));
    assert_eq!(row.cell_value(0), Value::Null);
    assert!(!row.cell_is_loaded(1));
    assert_eq!(row.cell_value(1), Value::Undecodable("not fetched".into()));
    assert!(!row.cells_are_complete());
    assert_eq!(row.complete_cells(), None);
}

#[test]
fn unloaded_result_row_cannot_be_copied_as_complete_data() {
    let row = RowObject::unloaded(3);
    assert_eq!(row.loaded_cells(), vec![None, None, None]);
    assert!(!row.cells_are_complete());
    assert!(row.complete_cells().is_none());
    let cloned = row.clone_preserving_loading_state();
    assert!(cloned.complete_cells().is_none());
}

#[test]
fn projected_cells_refuse_mismatched_or_duplicate_mappings() {
    assert!(RowObject::new_projected(vec![Value::Int(1)], &[], 1).is_none());
    assert!(RowObject::new_projected(vec![Value::Int(1), Value::Int(2)], &[0, 0], 1).is_none());
}

#[test]
fn cloning_a_projected_row_keeps_unfetched_cells_unloaded() {
    let row = RowObject::new_projected(vec![Value::Int(1)], &[0], 2).unwrap();
    let clone = row.clone_preserving_loading_state();

    assert_eq!(clone.cell_value(0), Value::Int(1));
    assert!(!clone.cell_is_loaded(1));
    assert!(clone.complete_cells().is_none());
}

#[test]
fn long_cells_keep_an_eight_kib_preview_and_primary_keys_remain_loaded() {
    let text = format!("{}é", "x".repeat(8191));
    let row = RowObject::new(vec![Value::Int(7), Value::Text(text.clone())]);
    row.preview_long_values(&[0]);

    assert_eq!(row.cell_value(0), Value::Int(7));
    assert_eq!(row.cell_value(1), Value::Text(text.clone()));
    let preview = row.cell_preview(1).unwrap();
    assert_eq!(preview.value, Value::Text("x".repeat(8191)));
    assert_eq!(preview.byte_count, PreviewByteCount::Exact(text.len()));
    assert_eq!(row.complete_cells(), Some(vec![Value::Int(7), Value::Text(text)]));
}

#[test]
fn previews_keep_the_byte_boundary_and_cover_json_and_binary_values() {
    let exact = RowObject::new(vec![Value::Int(1), Value::Text("x".repeat(8192))]);
    exact.preview_long_values(&[0]);
    assert!(exact.cell_is_loaded(1));

    let bytes = vec![0xA5; 8193];
    let row = RowObject::new(vec![
        Value::Int(1),
        Value::Bytes(bytes.clone()),
        Value::Json(Box::new(serde_json::json!("x".repeat(8192)))),
    ]);
    row.preview_long_values(&[0]);
    assert_eq!(row.cell_preview(1).unwrap().value, Value::Bytes(bytes[..8192].to_vec()));
    assert_eq!(row.cell_preview(1).unwrap().byte_count, PreviewByteCount::Exact(8193));
    assert!(row.cell_preview(2).is_some());
}
