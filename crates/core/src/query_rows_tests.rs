use super::QueryRows;
use crate::Value;

#[test]
fn rows_share_one_cell_buffer_and_keep_ragged_boundaries() {
    let rows = QueryRows::from_rows(vec![
        vec![Value::Int(1), Value::Text("one".into())],
        vec![Value::Int(2)],
        Vec::new(),
    ]);

    assert_eq!(rows.len(), 3);
    assert_eq!(rows.cell_count(), 3);
    assert_eq!(rows[0], [Value::Int(1), Value::Text("one".into())]);
    assert_eq!(rows[1], [Value::Int(2)]);
    assert!(rows[2].is_empty());
    assert_eq!(rows.starts, [0, 2, 3]);
}

#[test]
fn mutable_row_access_updates_cells_without_changing_boundaries() {
    let mut rows = QueryRows::from_rows(vec![vec![Value::Int(1)], vec![Value::Int(2), Value::Int(3)]]);

    if let Some(row) = rows.get_mut(0) {
        row[0] = Value::Null;
    }
    if let Some(row) = rows.get_mut(1) {
        row[0] = Value::Null;
    }

    assert_eq!(rows[0], [Value::Null]);
    assert_eq!(rows[1], [Value::Null, Value::Int(3)]);
}

#[test]
fn removing_a_row_shifts_later_cell_offsets() {
    let mut rows = QueryRows::from_rows(vec![
        vec![Value::Int(1)],
        vec![Value::Int(2), Value::Int(3)],
        vec![Value::Int(4)],
    ]);

    assert_eq!(rows.remove(1), [Value::Int(2), Value::Int(3)]);

    assert_eq!(rows[0], [Value::Int(1)]);
    assert_eq!(rows[1], [Value::Int(4)]);
    assert_eq!(rows.starts, [0, 1]);
}

#[test]
fn mutable_iteration_visits_each_row_once() {
    let mut rows = QueryRows::from_rows(vec![
        vec![Value::Int(1), Value::Int(2)],
        Vec::new(),
        vec![Value::Int(3)],
    ]);

    for row in rows.iter_mut() {
        for value in row {
            *value = Value::Null;
        }
    }

    assert_eq!(rows[0], [Value::Null, Value::Null]);
    assert!(rows[1].is_empty());
    assert_eq!(rows[2], [Value::Null]);
}

#[test]
fn reversing_rows_preserves_cell_order_inside_each_row() {
    let mut rows = QueryRows::from_rows(vec![
        vec![Value::Int(1), Value::Int(2)],
        vec![Value::Int(3)],
        vec![Value::Int(4), Value::Int(5), Value::Int(6)],
    ]);

    rows.reverse();

    assert_eq!(
        rows,
        vec![
            vec![Value::Int(4), Value::Int(5), Value::Int(6)],
            vec![Value::Int(3)],
            vec![Value::Int(1), Value::Int(2)],
        ]
    );
    assert_eq!(
        rows,
        QueryRows::from_rows(vec![
            vec![Value::Int(4), Value::Int(5), Value::Int(6)],
            vec![Value::Int(3)],
            vec![Value::Int(1), Value::Int(2)],
        ])
    );
    assert_eq!(rows[0], [Value::Int(4), Value::Int(5), Value::Int(6)]);
    rows[0][0] = Value::Int(40);
    assert_eq!(rows[0][0], Value::Int(40));
}

#[test]
fn appending_after_reversing_rows_keeps_logical_append_order() {
    let mut rows = QueryRows::from_rows(vec![vec![Value::Int(1)], vec![Value::Int(2)]]);

    rows.reverse();
    rows.push(vec![Value::Int(3)]);

    assert_eq!(
        rows,
        vec![vec![Value::Int(2)], vec![Value::Int(1)], vec![Value::Int(3)]]
    );
}

#[test]
fn truncating_after_reversing_rows_keeps_the_logical_prefix() {
    let mut rows = QueryRows::from_rows(vec![
        vec![Value::Int(1)],
        vec![Value::Int(2), Value::Int(3)],
        vec![Value::Int(4)],
    ]);

    rows.reverse();
    rows.truncate(2);

    assert_eq!(rows, vec![vec![Value::Int(4)], vec![Value::Int(2), Value::Int(3)]]);
}

#[test]
fn removing_after_reversing_rows_removes_the_logical_row() {
    let mut rows = QueryRows::from_rows(vec![
        vec![Value::Int(1)],
        vec![Value::Int(2), Value::Int(3)],
        vec![Value::Int(4)],
    ]);

    rows.reverse();
    assert_eq!(rows.remove(1), [Value::Int(2), Value::Int(3)]);

    assert_eq!(rows, vec![vec![Value::Int(4)], vec![Value::Int(1)]]);
}

#[test]
fn truncating_a_row_after_reversing_rows_uses_the_logical_index() {
    let mut rows = QueryRows::from_rows(vec![
        vec![Value::Int(1)],
        vec![Value::Int(2), Value::Int(3)],
        vec![Value::Int(4)],
    ]);

    rows.reverse();
    rows.truncate_row(1, 1);

    assert_eq!(
        rows,
        vec![vec![Value::Int(4)], vec![Value::Int(2)], vec![Value::Int(1)]]
    );
}

#[test]
fn mutable_iteration_after_reversing_rows_follows_logical_order() {
    let mut rows = QueryRows::from_rows(vec![
        vec![Value::Int(1)],
        vec![Value::Int(2), Value::Int(3)],
        vec![Value::Int(4)],
    ]);

    rows.reverse();
    for row in rows.iter_mut() {
        row[0] = Value::Null;
    }

    assert_eq!(
        rows,
        vec![vec![Value::Null], vec![Value::Null, Value::Int(3)], vec![Value::Null]]
    );
}

#[test]
fn reversing_empty_rows_preserves_their_positions() {
    let mut rows = QueryRows::from_rows(vec![
        Vec::new(),
        vec![Value::Int(1)],
        Vec::new(),
        vec![Value::Int(2), Value::Int(3)],
        Vec::new(),
    ]);

    rows.reverse();

    assert_eq!(
        rows.into_rows(),
        vec![
            Vec::new(),
            vec![Value::Int(2), Value::Int(3)],
            Vec::new(),
            vec![Value::Int(1)],
            Vec::new(),
        ]
    );
}

#[test]
fn truncating_rows_and_cells_keeps_offsets_valid() {
    let mut rows = QueryRows::from_rows(vec![
        vec![Value::Int(1), Value::Int(2)],
        vec![Value::Int(3), Value::Int(4), Value::Int(5)],
        vec![Value::Int(6)],
    ]);

    rows.truncate_row(1, 1);
    assert_eq!(rows[1], [Value::Int(3)]);
    assert_eq!(rows[2], [Value::Int(6)]);
    rows.truncate(2);

    assert_eq!(rows, vec![vec![Value::Int(1), Value::Int(2)], vec![Value::Int(3)]]);
    assert_eq!(rows.cell_count(), 3);
}

#[test]
fn wide_rows_and_exact_typed_values_survive_flattening() {
    let expected = (0..256).map(Value::Int).collect::<Vec<_>>();
    let typed = vec![
        Value::Null,
        Value::Bool(true),
        Value::Int(i64::MIN),
        Value::Text(String::new()),
        Value::Bytes(vec![0, 255, 0]),
        Value::Json(Box::new(serde_json::json!({"exact": "001.2300"}))),
        Value::Undecodable("numeric(38, 20)".into()),
    ];
    let rows = QueryRows::from_rows(vec![expected.clone(), typed.clone()]);

    assert_eq!(rows[0], expected);
    assert_eq!(rows[1], typed);
    assert_eq!(rows.into_rows(), vec![expected, typed]);
}
