use super::*;

#[test]
fn snapshot_is_send_and_rejects_input_before_retaining_it() {
    fn assert_send<T: Send>() {}
    assert_send::<ReplaceSnapshot>();
    let mut snapshot = ReplaceSnapshot::default();
    let value = Value::Text("x".repeat(16 * 1024 * 1024));
    assert!(matches!(
        snapshot.push(0, &[Value::Int(1)], &[(0, &value)]),
        Err(ReplaceError::Budget)
    ));
    assert!(snapshot.rows().is_empty());
    snapshot
        .push(3, &[Value::Int(2)], &[(4, &Value::Text("small".into()))])
        .unwrap();
    assert_eq!(snapshot.rows()[0].position, 3);
    assert_eq!(snapshot.rows()[0].cells, vec![(4, Value::Text("small".into()))]);
}

#[test]
fn an_oversized_cell_discards_earlier_cells_from_the_same_snapshot_row() {
    let mut snapshot = ReplaceSnapshot::default();
    let first = Value::Text("small".into());
    let oversized = Value::Text("x".repeat(16 * 1024 * 1024));
    assert!(matches!(
        snapshot.push(0, &[Value::Int(1)], &[(0, &first), (1, &oversized)]),
        Err(ReplaceError::Budget)
    ));
    assert!(snapshot.rows().is_empty());
}

#[test]
fn snapshot_plan_keeps_original_values_for_stale_cell_validation() {
    let mut snapshot = ReplaceSnapshot::default();
    let original = Value::Text("old value".into());
    snapshot.push(8, &[Value::Int(42)], &[(2, &original)]).unwrap();
    let query = ReplaceQuery {
        needle: "old".into(),
        options: Default::default(),
        column: None,
    };
    let report = plan_snapshot_replacements(
        &snapshot,
        ReplaceRequest {
            query: &query,
            replacement: "new",
            editable: &[false, false, true],
        },
        &CancellationToken::new(),
        |_| String::new(),
    )
    .unwrap();
    assert_eq!(report.edits.len(), 1);
    assert_eq!(report.edits[0].position, 8);
    assert_eq!(report.edits[0].row_key, vec![Value::Int(42)]);
    assert_eq!(report.edits[0].original, original);
}

#[test]
fn replacement_input_above_limit_is_rejected_before_planning() {
    let query = ReplaceQuery {
        needle: "a".into(),
        options: Default::default(),
        column: None,
    };
    let result = plan_replacements(
        1,
        |_, _| panic!("over-budget input must be refused before reading rows"),
        |_| Some(vec![Value::Int(1)]),
        ReplaceRequest {
            query: &query,
            replacement: &"x".repeat(MAX_REPLACE_INPUT_BYTES + 1),
            editable: &[true],
        },
        &CancellationToken::new(),
        |_| String::new(),
    );
    assert!(matches!(result, Err(ReplaceError::Budget)));
}

#[test]
fn cancellation_on_the_last_row_discards_already_planned_edits() {
    let query = ReplaceQuery {
        needle: "a".into(),
        options: Default::default(),
        column: None,
    };
    let value = Value::Text("a".into());
    let cancel = CancellationToken::new();
    let result = plan_replacements(
        2,
        |position, visit| {
            visit(0, &value);
            if position == 1 {
                cancel.cancel();
            }
        },
        |position| Some(vec![Value::Int(i64::from(position))]),
        ReplaceRequest {
            query: &query,
            replacement: "b",
            editable: &[true],
        },
        &cancel,
        |_| String::new(),
    );
    assert!(matches!(result, Err(ReplaceError::Cancelled)));
}

#[test]
fn oversized_keys_do_not_escape_the_retained_plan_budget() {
    let query = ReplaceQuery {
        needle: "a".into(),
        options: Default::default(),
        column: None,
    };
    let value = Value::Text("a".into());
    let result = plan_replacements(
        17,
        |_, visit| visit(0, &value),
        |_| Some(vec![Value::Text("k".repeat(1024 * 1024))]),
        ReplaceRequest {
            query: &query,
            replacement: "b",
            editable: &[true],
        },
        &CancellationToken::new(),
        |_| String::new(),
    );
    assert!(matches!(result, Err(ReplaceError::Budget)));
}

#[test]
fn cancellation_returns_no_plan() {
    let query = ReplaceQuery {
        needle: "a".into(),
        options: Default::default(),
        column: None,
    };
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = plan_replacements(
        1,
        |_, _| panic!("cancelled rows must not be visited"),
        |_| Some(vec![Value::Int(1)]),
        ReplaceRequest {
            query: &query,
            replacement: "b",
            editable: &[true],
        },
        &cancel,
        |_| String::new(),
    );
    assert!(matches!(result, Err(ReplaceError::Cancelled)));
}

#[test]
fn expanded_cell_is_refused_without_a_partial_plan() {
    let query = ReplaceQuery {
        needle: "a".into(),
        options: Default::default(),
        column: None,
    };
    let value = Value::Text("a".repeat(1024));
    let replacement = "b".repeat(2048);
    let result = plan_replacements(
        1,
        |_, visit| visit(0, &value),
        |_| Some(vec![Value::Int(1)]),
        ReplaceRequest {
            query: &query,
            replacement: &replacement,
            editable: &[true],
        },
        &CancellationToken::new(),
        |_| String::new(),
    );
    assert!(matches!(result, Err(ReplaceError::Budget)));
}
