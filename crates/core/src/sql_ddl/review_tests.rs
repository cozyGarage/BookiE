use crate::query::{ColumnInfo, ForeignKeyInfo, IndexInfo};

use super::*;

fn info(name: &str, data_type: &str, nullable: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn index(name: &str, primary: bool) -> IndexInfo {
    IndexInfo {
        name: name.into(),
        columns: vec!["a".into()],
        unique: false,
        primary,
        predicate: None,
        constraint: None,
        has_expression_key: false,
        using_clause: None,
    }
}

fn key(name: &str) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.into(),
        columns: vec!["a".into()],
        ref_schema: None,
        ref_table: "other".into(),
        ref_columns: vec!["id".into()],
        on_delete: None,
        on_update: None,
    }
}

fn review(
    columns: &[ColumnInfo],
    drafts: &[DraftColumn],
    indexes: (&[IndexInfo], &[IndexInfo]),
    keys: (&[ForeignKeyInfo], &[ForeignKeyInfo]),
) -> Vec<DestructiveChange> {
    review_changes(
        &TableState {
            table: "t",
            columns,
            indexes: indexes.0,
            fks: keys.0,
        },
        &TableState {
            table: "t",
            columns: drafts,
            indexes: indexes.1,
            fks: keys.1,
        },
    )
}

#[test]
fn an_untouched_table_loses_nothing() {
    let columns = [info("a", "integer", true), info("b", "text", false)];
    let drafts: Vec<DraftColumn> = columns.iter().cloned().map(DraftColumn::from_info).collect();
    assert!(review(&columns, &drafts, (&[], &[]), (&[], &[])).is_empty());
}

#[test]
fn dropping_a_column_an_index_or_a_key_is_named() {
    let columns = [info("a", "integer", true), info("b", "text", true)];
    let drafts = vec![DraftColumn::from_info(columns[0].clone())];
    let found = review(
        &columns,
        &drafts,
        (&[index("ix_a", false), index("pk", true)], &[]),
        (&[key("fk_a")], &[]),
    );
    assert_eq!(
        found,
        vec![
            DestructiveChange::DropColumn { column: "b".into() },
            DestructiveChange::DropIndex { index: "ix_a".into() },
            DestructiveChange::DropForeignKey { key: "fk_a".into() },
        ],
        "a primary index is not dropped on its own"
    );
}

#[test]
fn a_type_change_and_a_new_not_null_are_named_but_a_rename_or_added_column_is_not() {
    let columns = [info("a", "integer", true), info("b", "text", true)];
    let mut drafts: Vec<DraftColumn> = columns.iter().cloned().map(DraftColumn::from_info).collect();
    drafts[0].data_type = " INTEGER ".into();
    drafts[0].name = "renamed".into();
    drafts[1].data_type = "varchar(10)".into();
    drafts[1].nullable = false;
    let mut added = DraftColumn::from_info(info("c", "text", false));
    added.original = None;
    drafts.push(added);
    assert_eq!(
        review(&columns, &drafts, (&[], &[]), (&[], &[])),
        vec![
            DestructiveChange::ChangeColumnType {
                column: "b".into(),
                from: "text".into(),
                to: "varchar(10)".into()
            },
            DestructiveChange::SetNotNull { column: "b".into() },
        ]
    );
}
