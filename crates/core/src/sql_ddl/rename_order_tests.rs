use crate::query::ColumnInfo;

use super::diff::diff_to_ops;
use super::{DraftColumn, StructureOp, materialize_ops};

fn existing(name: &str, data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
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

fn column_names_after_renames(original: &[ColumnInfo], ops: &[StructureOp]) -> Vec<String> {
    let mut names: Vec<String> = original.iter().map(|column| column.name.clone()).collect();
    for op in ops {
        match op {
            StructureOp::DropColumn { column_name, .. } => names.retain(|name| name != column_name),
            StructureOp::RenameColumn { old_name, new_name, .. } => {
                let at = names
                    .iter()
                    .position(|name| name == old_name)
                    .unwrap_or_else(|| panic!("renaming {old_name}, which does not exist: {ops:?}"));
                assert!(
                    !names.contains(new_name),
                    "renaming {old_name} to {new_name} while {new_name} still exists: {ops:?}"
                );
                names[at] = new_name.clone();
            }
            _ => {}
        }
    }
    names.sort();
    names
}

fn renamed_drafts(original: &[ColumnInfo], new_names: &[&str]) -> Vec<DraftColumn> {
    original
        .iter()
        .zip(new_names)
        .map(|(column, new_name)| {
            let mut draft = DraftColumn::from_info(column.clone());
            draft.name = (*new_name).into();
            draft
        })
        .collect()
}

#[test]
fn swapping_two_column_names_renames_through_a_free_name() {
    let original = vec![existing("a", "text"), existing("b", "text")];
    let drafts = renamed_drafts(&original, &["b", "a"]);

    let ops = diff_to_ops(None, "t", "t", &original, &drafts, &[], &[], &[], &[]);

    assert_eq!(column_names_after_renames(&original, &ops), vec!["a", "b"]);
    for driver in ["postgres", "mysql", "mssql"] {
        assert!(materialize_ops(&ops, driver).is_ok(), "{driver}");
    }
}

#[test]
fn a_chain_of_renames_runs_in_dependency_order_whatever_the_column_order() {
    let original = vec![existing("a", "text"), existing("b", "text")];
    let drafts = renamed_drafts(&original, &["b", "c"]);

    let ops = diff_to_ops(None, "t", "t", &original, &drafts, &[], &[], &[], &[]);

    assert_eq!(column_names_after_renames(&original, &ops), vec!["b", "c"]);
}

#[test]
fn a_dropped_columns_name_can_be_taken_by_another_column() {
    let original = vec![existing("x", "text"), existing("y", "text")];
    let drafts = vec![{
        let mut draft = DraftColumn::from_info(original[1].clone());
        draft.name = "x".into();
        draft
    }];

    let ops = diff_to_ops(None, "t", "t", &original, &drafts, &[], &[], &[], &[]);

    assert_eq!(column_names_after_renames(&original, &ops), vec!["x"]);
}
