use crate::query::ColumnInfo;

use super::diff::{TableState, same_index};
use super::types::DraftColumn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestructiveChange {
    DropColumn { column: String },
    ChangeColumnType { column: String, from: String, to: String },
    SetNotNull { column: String },
    DropIndex { index: String },
    DropForeignKey { key: String },
}

pub fn review_changes(
    original: &TableState<'_, ColumnInfo>,
    current: &TableState<'_, DraftColumn>,
) -> Vec<DestructiveChange> {
    let mut changes = Vec::new();
    for column in original.columns {
        let kept = current
            .columns
            .iter()
            .any(|draft| draft.original.as_ref().is_some_and(|o| o.name == column.name));
        if !kept {
            changes.push(DestructiveChange::DropColumn {
                column: column.name.clone(),
            });
        }
    }
    for draft in current.columns {
        let Some(before) = &draft.original else { continue };
        if !before.data_type.trim().eq_ignore_ascii_case(draft.data_type.trim()) {
            changes.push(DestructiveChange::ChangeColumnType {
                column: before.name.clone(),
                from: before.data_type.clone(),
                to: draft.data_type.clone(),
            });
        }
        if before.nullable && !draft.nullable {
            changes.push(DestructiveChange::SetNotNull {
                column: before.name.clone(),
            });
        }
    }
    for index in original.indexes {
        if !index.primary && !current.indexes.iter().any(|kept| same_index(kept, index)) {
            changes.push(DestructiveChange::DropIndex {
                index: index.name.clone(),
            });
        }
    }
    for key in original.fks {
        if !current.fks.contains(key) {
            changes.push(DestructiveChange::DropForeignKey { key: key.name.clone() });
        }
    }
    changes
}
