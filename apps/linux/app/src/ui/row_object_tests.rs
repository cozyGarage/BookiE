use super::*;
use gtk4::glib::object::CastNone;
use gtk4::prelude::ListModelExt;
use std::sync::Arc;
use tablepro_core::{ColumnInfo, QueryResult};

#[test]
fn json_preview_writer_counts_all_bytes_without_growing_past_its_sample() {
    let mut writer = JsonPreviewWriter {
        prefix: Vec::with_capacity(PREVIEW_BYTES),
        byte_count: 0,
    };
    let chunk = vec![b'x'; PREVIEW_BYTES * 8];
    std::io::Write::write_all(&mut writer, &chunk).unwrap();
    std::io::Write::write_all(&mut writer, &chunk).unwrap();
    assert_eq!(writer.byte_count, PREVIEW_BYTES * 16);
    assert_eq!(writer.prefix, vec![b'x'; PREVIEW_BYTES]);
    assert_eq!(writer.prefix.capacity(), PREVIEW_BYTES);
}

#[test]
fn json_preview_writer_refuses_an_overflowing_byte_count() {
    let mut writer = JsonPreviewWriter {
        prefix: Vec::new(),
        byte_count: usize::MAX,
    };
    let error = std::io::Write::write(&mut writer, b"x").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(writer.byte_count, usize::MAX);
    assert!(writer.prefix.is_empty());
}

#[test]
fn a_large_parsed_json_preview_keeps_its_sample_and_exact_serialized_byte_count() {
    let value = Value::Json(Box::new(serde_json::Value::String("x".repeat(8 * 1024 * 1024))));
    let preview = preview_value(&value).unwrap();
    assert_eq!(preview.byte_count, 8 * 1024 * 1024 + 2);
    assert_eq!(preview.value, Value::Text(format!("\"{}", "x".repeat(8191))));
    assert!(
        matches!(&value, Value::Json(json) if matches!(json.as_ref(), serde_json::Value::String(text) if text.len() == 8 * 1024 * 1024))
    );
}

#[test]
fn parsed_json_preview_matches_serialized_escapes_and_utf8_boundaries() {
    let json = serde_json::json!({"payload": "é😀\0\\\"".repeat(2000)});
    let serialized = json.to_string();
    let preview = preview_value(&Value::Json(Box::new(json))).unwrap();
    let end = serialized.floor_char_boundary(8192);
    assert_eq!(preview.byte_count, serialized.len());
    assert_eq!(preview.value, Value::Text(serialized[..end].to_owned()));
}

#[test]
fn parsed_json_uses_its_serialized_size_at_the_preview_boundary() {
    assert!(preview_value(&Value::Json(Box::new(serde_json::Value::String("x".repeat(8190))))).is_none());
    let preview = preview_value(&Value::Json(Box::new(serde_json::Value::String("x".repeat(8191))))).unwrap();
    assert_eq!(preview.byte_count, 8193);
}

#[test]
fn a_json_value_is_measured_without_serialising_small_documents_twice() {
    assert!(!json_exceeds(&serde_json::json!({"a": 1}), 8));
    assert!(json_exceeds(&serde_json::json!({"payload": "x".repeat(20)}), 8));
    assert!(!json_exceeds(&serde_json::json!("12345"), 7));
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
        rows: vec![vec![Value::Text(payload.clone())]],
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
    assert_eq!(row.cell_preview(0).unwrap().byte_count, payload.len());
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
    assert_eq!(preview.byte_count, text.len());
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
    assert_eq!(row.cell_preview(1).unwrap().byte_count, 8193);
    assert!(row.cell_preview(2).is_some());
}
