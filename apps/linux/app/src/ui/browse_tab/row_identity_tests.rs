use super::{DuplicateRowError, duplicate_row_values, row_key_matches};
use bookie_core::{ColumnInfo, Value};

fn column(name: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "integer".into(),
        nullable: false,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn matches_cells(columns: &[ColumnInfo], cells: &[Value], expected: &[Value]) -> bool {
    let row = crate::ui::row_object::RowObject::new(cells.to_vec());
    row_key_matches(columns, &row, expected)
}

#[test]
fn row_identity_checks_require_loaded_keys_but_not_loaded_payloads() {
    let columns = [column("id", true), column("payload", false)];
    let keyed = crate::ui::row_object::RowObject::new_projected(vec![Value::Int(7)], &[0], 2).unwrap();
    assert!(row_key_matches(&columns, &keyed, &[Value::Int(7)]));
    let key_missing =
        crate::ui::row_object::RowObject::new_projected(vec![Value::Text("payload".into())], &[1], 2).unwrap();
    assert!(!row_key_matches(&columns, &key_missing, &[Value::Int(7)]));
}

#[test]
fn shared_row_identity_uses_current_key_values_and_refuses_a_released_source() {
    let columns = [column("id", true), column("payload", false)];
    let result = std::sync::Arc::new(bookie_core::QueryResult {
        columns: columns.to_vec(),
        rows: vec![vec![Value::Int(7), Value::Text("payload".into())]].into(),
        truncated: false,
    });
    let row = crate::ui::row_object::RowObject::from_shared(&result, 0, None).unwrap();
    assert!(row_key_matches(&columns, &row, &[Value::Int(7)]));
    row.set_cell(0, Value::Int(8));
    assert!(!row_key_matches(&columns, &row, &[Value::Int(7)]));
    assert!(row_key_matches(&columns, &row, &[Value::Int(8)]));
    let released = crate::ui::row_object::RowObject::from_shared(&result, 0, None).unwrap();
    drop(result);
    assert!(!row_key_matches(&columns, &released, &[Value::Int(7)]));
    assert!(row_key_matches(&columns, &row, &[Value::Int(8)]));
}

#[test]
fn wide_row_identity_checks_preserve_the_non_key_payload() {
    let columns = [column("id", true), column("payload", false)];
    let row = crate::ui::row_object::RowObject::new(vec![Value::Int(7), Value::Text("x".repeat(16 * 1024 * 1024))]);
    for _ in 0..8 {
        assert!(row_key_matches(&columns, &row, &[Value::Int(7)]));
    }
    assert!(!row_key_matches(&columns, &row, &[Value::Int(8)]));
    assert_eq!(
        row.with_cell_value(1, |value| match value {
            Value::Text(text) => text.len(),
            _ => 0,
        }),
        Some(16 * 1024 * 1024)
    );
}

#[test]
fn the_same_row_still_matches_its_key() {
    let columns = [column("id", true), column("note", false)];
    let cells = [Value::Int(7), Value::Text("a".into())];
    assert!(matches_cells(&columns, &cells, &[Value::Int(7)]));
}

#[test]
fn a_different_row_at_the_same_position_is_refused() {
    let columns = [column("id", true), column("note", false)];
    let cells = [Value::Int(8), Value::Text("a".into())];
    assert!(!matches_cells(&columns, &cells, &[Value::Int(7)]));
}

#[test]
fn editing_another_column_of_the_same_row_still_matches() {
    let columns = [column("id", true), column("note", false)];
    let cells = [Value::Int(7), Value::Text("edited".into())];
    assert!(matches_cells(&columns, &cells, &[Value::Int(7)]));
}

#[test]
fn every_component_of_a_composite_key_has_to_match() {
    let columns = [column("a", true), column("b", true), column("c", false)];
    assert!(matches_cells(
        &columns,
        &[Value::Int(1), Value::Int(2), Value::Int(9)],
        &[Value::Int(1), Value::Int(2)]
    ));
    assert!(!matches_cells(
        &columns,
        &[Value::Int(1), Value::Int(3), Value::Int(9)],
        &[Value::Int(1), Value::Int(2)]
    ));
}

#[test]
fn a_key_that_lost_or_gained_a_component_is_refused() {
    let columns = [column("a", true), column("b", true)];
    assert!(!matches_cells(
        &columns,
        &[Value::Int(1), Value::Int(2)],
        &[Value::Int(1)]
    ));
    let one_key = [column("a", true), column("b", false)];
    assert!(!matches_cells(
        &one_key,
        &[Value::Int(1), Value::Int(2)],
        &[Value::Int(1), Value::Int(2)]
    ));
}

#[test]
fn a_null_key_component_compares_by_value_like_any_other() {
    let columns = [column("a", true), column("b", false)];
    assert!(matches_cells(&columns, &[Value::Null, Value::Int(1)], &[Value::Null]));
    assert!(!matches_cells(
        &columns,
        &[Value::Int(0), Value::Int(1)],
        &[Value::Null]
    ));
}

#[test]
fn a_table_without_a_primary_key_has_nothing_to_compare() {
    let columns = [column("a", false)];
    assert!(matches_cells(&columns, &[Value::Int(1)], &[]));
}

#[test]
fn duplicating_a_row_blanks_database_owned_cells_and_preserves_exact_values() {
    let columns = [column("id", true), column("amount", false), column("note", false)];
    let source = [
        Value::Int(7),
        Value::Decimal("12.30".parse().unwrap()),
        Value::Text("keep me".into()),
    ];
    assert_eq!(
        duplicate_row_values(&columns, &source),
        Ok(vec![
            Value::Null,
            Value::Decimal("12.30".parse().unwrap()),
            Value::Text("keep me".into())
        ])
    );
}

#[test]
fn duplicating_a_row_refuses_an_undecodable_cell_instead_of_turning_it_into_null() {
    let columns = [column("id", true), column("amount", false)];
    let source = [Value::Int(7), Value::Undecodable("NUMERIC".into())];
    assert_eq!(
        duplicate_row_values(&columns, &source),
        Err(DuplicateRowError::Undecodable {
            column: "amount".into()
        })
    );
}

#[test]
fn duplicating_a_row_carries_the_full_long_text_into_the_insert_not_the_display_text() {
    let long_text = "é".repeat(250_000);
    let long_utf8_bytes = "z".repeat(90_000).into_bytes();
    let columns = [column("id", true), column("body", false), column("raw", false)];
    let source = [
        Value::Int(1),
        Value::Text(long_text.clone()),
        Value::Bytes(long_utf8_bytes.clone()),
    ];
    assert_ne!(
        crate::ui::grid::value_to_display_text(&source[1]),
        long_text,
        "the value must be long enough for the grid to truncate it"
    );

    let values = duplicate_row_values(&columns, &source).unwrap();
    assert_eq!(values[1], Value::Text(long_text.clone()));
    assert_eq!(values[2], Value::Bytes(long_utf8_bytes.clone()));

    let mut tracker = crate::services::change_tracker::TabChangeTracker::new();
    tracker.track_insert(values);
    let (statements, _) = tracker.materialize("mysql", None, "t", &columns).unwrap();
    assert_eq!(
        statements[0].1,
        vec![Value::Null, Value::Text(long_text), Value::Bytes(long_utf8_bytes)]
    );
}

#[test]
fn duplicating_a_row_refuses_a_source_with_wrong_cell_count() {
    let columns = [column("id", true), column("note", false)];
    for source in [
        &[Value::Int(1)][..],
        &[Value::Int(1), Value::Text("ok".into()), Value::Int(3)][..],
    ] {
        let actual = source.len();
        assert_eq!(
            duplicate_row_values(&columns, source),
            Err(DuplicateRowError::CellCountMismatch { expected: 2, actual })
        );
    }
}

#[test]
fn a_row_shorter_than_its_columns_is_refused_rather_than_assumed_equal() {
    let columns = [column("a", true), column("b", true)];
    assert!(!matches_cells(
        &columns,
        &[Value::Int(1)],
        &[Value::Int(1), Value::Int(2)]
    ));
}
