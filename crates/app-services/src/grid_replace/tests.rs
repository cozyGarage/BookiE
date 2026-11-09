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
fn cancellation_during_snapshot_capture_discards_the_partial_row() {
    let mut snapshot = ReplaceSnapshot::default();
    let cancel = CancellationToken::new();
    let first = Value::Text("first".into());
    let second = Value::Text("second".into());
    let result = snapshot.push_row_cancellable(0, &[Value::Int(1)], Some(&cancel), |visit| {
        visit(0, &first);
        cancel.cancel();
        visit(1, &second);
    });
    assert!(matches!(result, Err(ReplaceError::Cancelled)));
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
fn stale_replacement_plan_rejects_the_whole_batch_before_staging() {
    let report = ReplaceReport {
        edits: vec![
            ReplaceEdit {
                position: 0,
                column: 1,
                original: Value::Text("old first".into()),
                new_value: "new first".into(),
                matches: 1,
                row_key: vec![Value::Int(1)],
            },
            ReplaceEdit {
                position: 1,
                column: 1,
                original: Value::Text("old second".into()),
                new_value: "new second".into(),
                matches: 1,
                row_key: vec![Value::Int(2)],
            },
        ],
        replacements: 2,
        skipped: 0,
        retained_bytes: 0,
    };
    let current_rows = [
        (
            vec![Value::Int(1)],
            vec![Value::Int(1), Value::Text("old first".into())],
        ),
        (vec![Value::Int(2)], vec![Value::Int(2), Value::Text("changed".into())]),
    ];
    let mut staged = Vec::new();
    let validation = validate_replace_plan(&report, |position, edits| {
        let (key, cells) = current_rows.get(position as usize)?;
        let cells = edits
            .iter()
            .map(|edit| cells.get(edit.column).cloned())
            .collect::<Option<Vec<_>>>()?;
        Some((key.clone(), cells))
    });
    if validation.is_ok() {
        staged.extend(report.edits.iter().map(|edit| edit.new_value.as_str()));
    }
    assert!(matches!(validation, Err(ReplaceError::Stale)));
    assert!(staged.is_empty());
}

#[test]
fn replacement_plan_validation_accepts_only_matching_original_values_and_keys() {
    let report = ReplaceReport {
        edits: vec![ReplaceEdit {
            position: 0,
            column: 1,
            original: Value::Text("old".into()),
            new_value: "new".into(),
            matches: 1,
            row_key: vec![Value::Int(4)],
        }],
        replacements: 1,
        skipped: 0,
        retained_bytes: 0,
    };
    let result = validate_replace_plan(&report, |_, edits| {
        assert_eq!(edits.iter().map(|edit| edit.column).collect::<Vec<_>>(), [1]);
        Some((vec![Value::Int(4)], vec![Value::Text("old".into())]))
    });
    assert!(result.is_ok());
}

#[test]
fn invalid_later_replacement_value_refuses_the_prepared_batch() {
    let report = ReplaceReport {
        edits: vec![
            ReplaceEdit {
                position: 0,
                column: 1,
                original: Value::Text("first".into()),
                new_value: "valid".into(),
                matches: 1,
                row_key: vec![Value::Int(1)],
            },
            ReplaceEdit {
                position: 0,
                column: 2,
                original: Value::Text("second".into()),
                new_value: "invalid".into(),
                matches: 1,
                row_key: vec![Value::Int(1)],
            },
        ],
        replacements: 2,
        skipped: 0,
        retained_bytes: 0,
    };
    let current = (
        vec![Value::Int(1)],
        vec![Value::Int(1), Value::Text("first".into()), Value::Text("second".into())],
    );
    let result = prepare_replace_plan(
        &report,
        |_, edits| {
            let cells = edits
                .iter()
                .map(|edit| current.1.get(edit.column).cloned())
                .collect::<Option<Vec<_>>>()?;
            Some((current.0.clone(), cells))
        },
        |column, _, value| {
            if column == 2 {
                Err(ReplaceApplyError::InvalidValue)
            } else {
                Ok(Value::Text(value.into()))
            }
        },
    );
    assert!(matches!(result, Err(ReplaceApplyError::InvalidValue)));
}

#[test]
fn prepared_batch_counts_only_cells_that_change_typed_values() {
    let report = ReplaceReport {
        edits: vec![
            ReplaceEdit {
                position: 0,
                column: 1,
                original: Value::Text("same".into()),
                new_value: "same".into(),
                matches: 2,
                row_key: vec![Value::Int(1)],
            },
            ReplaceEdit {
                position: 0,
                column: 2,
                original: Value::Text("before".into()),
                new_value: "after".into(),
                matches: 3,
                row_key: vec![Value::Int(1)],
            },
        ],
        replacements: 5,
        skipped: 0,
        retained_bytes: 0,
    };
    let current = (
        vec![Value::Int(1)],
        vec![Value::Int(1), Value::Text("same".into()), Value::Text("before".into())],
    );
    let prepared = prepare_replace_plan(
        &report,
        |_, edits| {
            let cells = edits
                .iter()
                .map(|edit| current.1.get(edit.column).cloned())
                .collect::<Option<Vec<_>>>()?;
            Some((current.0.clone(), cells))
        },
        |_, _, value| Ok(Value::Text(value.into())),
    )
    .unwrap();
    assert_eq!(prepared.edits.len(), 1);
    assert_eq!(prepared.edits[0].column, 2);
    assert_eq!(prepared.replacements, 3);
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
