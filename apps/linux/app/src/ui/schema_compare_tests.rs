use super::*;

fn table(schema: Option<&str>, name: &str) -> TableInfo {
    TableInfo {
        schema: schema.map(str::to_string),
        name: name.into(),
    }
}

#[test]
fn the_choices_are_the_other_schemas_once_each_and_sorted() {
    let tables = [
        table(Some("b"), "x"),
        table(Some("a"), "y"),
        table(Some("b"), "z"),
        table(Some("c"), "w"),
        table(None, "plain"),
    ];
    assert_eq!(other_schemas(&tables, Some("a")), ["b", "c"]);
    assert!(other_schemas(&[table(Some("a"), "x")], Some("a")).is_empty());
}

#[test]
fn only_the_tables_of_the_chosen_schema_are_read() {
    let tables = [table(Some("a"), "x"), table(Some("b"), "y")];
    let names: Vec<_> = tables_in(&tables, Some("b"))
        .into_iter()
        .map(|table| table.name)
        .collect();
    assert_eq!(names, ["y"]);
}

#[test]
fn an_incomplete_read_refuses_the_comparison_instead_of_reporting_false_drops() {
    let complete = SchemaRead::default();
    assert!(both_complete(&complete, &complete).is_ok());
    let truncated = SchemaRead {
        total_tables: 200,
        ..SchemaRead::default()
    };
    assert!(
        both_complete(&complete, &truncated)
            .unwrap_err()
            .contains("more tables")
    );
    let unreadable = SchemaRead {
        total_tables: 1,
        skipped: vec!["secret_table".into()],
        ..SchemaRead::default()
    };
    assert!(
        both_complete(&unreadable, &complete)
            .unwrap_err()
            .contains("secret_table")
    );
}

#[test]
fn the_summary_counts_each_kind_and_says_when_the_schemas_match() {
    assert!(summary(&SchemaCompare::default()).contains("match"));
    let result = SchemaCompare {
        added: vec!["a".into()],
        dropped: vec!["b".into(), "c".into()],
        ..SchemaCompare::default()
    };
    assert_eq!(
        summary(&result),
        "1 new, 0 changed and 2 dropped tables. The script is open in a new editor tab for review."
    );
}
