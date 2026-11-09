use super::*;

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
