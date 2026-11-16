use bookie_core::{ColumnInfo, Value};

use crate::change_tracker::{KeyValue, pk_values_are_unreadable};
use crate::value_input::{ParseHooks, ValueInputError, parse_grid_cell_input_with_style};

pub const MAX_SET_CELLS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetCellsValue {
    Null,
    Text(String),
}

#[derive(Debug, Clone)]
pub struct SetCellTarget {
    pub position: u32,
    pub column: usize,
    pub row_key: Option<Vec<Value>>,
    pub original: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedCellEdit {
    pub position: u32,
    pub column: usize,
    pub row_key: Vec<Value>,
    pub original: Value,
    pub value: Value,
}

#[derive(Debug, Default, PartialEq)]
pub struct SetCellsPlan {
    pub edits: Vec<PlannedCellEdit>,
    pub unchanged: usize,
    pub skipped: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SetCellsError {
    TableNotEditable,
    NothingSelected,
    TooManyCells,
    ReadOnlyColumn { column: String },
    NotNullable { column: String },
    NotEditable { column: String, position: u32 },
    InvalidValue { column: String, error: ValueInputError },
}

pub struct SetCellsRequest<'a> {
    pub columns: &'a [ColumnInfo],
    pub table_editable: bool,
    pub value: &'a SetCellsValue,
    pub targets: &'a [SetCellTarget],
    pub hooks: &'a ParseHooks,
    pub cell_allows_edit: fn(&ColumnInfo, &Value) -> bool,
}

pub fn plan_set_cells(request: SetCellsRequest<'_>) -> Result<SetCellsPlan, SetCellsError> {
    if !request.table_editable {
        return Err(SetCellsError::TableNotEditable);
    }
    if request.targets.is_empty() {
        return Err(SetCellsError::NothingSelected);
    }
    if request.targets.len() > MAX_SET_CELLS {
        return Err(SetCellsError::TooManyCells);
    }
    check_columns(&request)?;
    let mut targets: Vec<&SetCellTarget> = request.targets.iter().collect();
    targets.sort_by_key(|target| (target.position, target.column));
    let mut plan = SetCellsPlan::default();
    for target in targets {
        plan_target(&request, target, &mut plan)?;
    }
    Ok(plan)
}

fn check_columns(request: &SetCellsRequest<'_>) -> Result<(), SetCellsError> {
    let mut seen = std::collections::BTreeSet::new();
    for target in request.targets {
        if !seen.insert(target.column) {
            continue;
        }
        let Some(column) = request.columns.get(target.column) else {
            return Err(SetCellsError::NotEditable {
                column: String::new(),
                position: target.position,
            });
        };
        if column.primary_key || column.is_generated || column.is_auto_increment {
            return Err(SetCellsError::ReadOnlyColumn {
                column: column.name.clone(),
            });
        }
        if matches!(request.value, SetCellsValue::Null) && !column.nullable {
            return Err(SetCellsError::NotNullable {
                column: column.name.clone(),
            });
        }
    }
    Ok(())
}

fn plan_target(
    request: &SetCellsRequest<'_>,
    target: &SetCellTarget,
    plan: &mut SetCellsPlan,
) -> Result<(), SetCellsError> {
    let Some(column) = request.columns.get(target.column) else {
        return Ok(());
    };
    let Some(row_key) = target
        .row_key
        .as_ref()
        .filter(|key| !key.is_empty() && !pk_values_are_unreadable(key))
    else {
        plan.skipped += 1;
        return Ok(());
    };
    if !(request.cell_allows_edit)(column, &target.original) {
        return Err(SetCellsError::NotEditable {
            column: column.name.clone(),
            position: target.position,
        });
    }
    let value = match request.value {
        SetCellsValue::Null => Value::Null,
        SetCellsValue::Text(text) => {
            parse_grid_cell_input_with_style(text, Some(column), request.hooks, Some(&target.original)).map_err(
                |error| SetCellsError::InvalidValue {
                    column: column.name.clone(),
                    error,
                },
            )?
        }
    };
    if KeyValue::from(&value) == KeyValue::from(&target.original) {
        plan.unchanged += 1;
        return Ok(());
    }
    plan.edits.push(PlannedCellEdit {
        position: target.position,
        column: target.column,
        row_key: row_key.clone(),
        original: target.original.clone(),
        value,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::EnumInputStyle;

    fn column(name: &str, data_type: &str, nullable: bool) -> ColumnInfo {
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

    fn allows(column: &ColumnInfo, _: &Value) -> bool {
        !column.name.starts_with("ro_")
    }

    fn target(position: u32, column: usize, original: Value) -> SetCellTarget {
        SetCellTarget {
            position,
            column,
            row_key: Some(vec![Value::Int(i64::from(position))]),
            original,
        }
    }

    fn plan(
        columns: &[ColumnInfo],
        editable: bool,
        value: SetCellsValue,
        targets: &[SetCellTarget],
    ) -> Result<SetCellsPlan, SetCellsError> {
        let hooks = ParseHooks::without_driver(EnumInputStyle::RawValue);
        plan_set_cells(SetCellsRequest {
            columns,
            table_editable: editable,
            value: &value,
            targets,
            hooks: &hooks,
            cell_allows_edit: allows,
        })
    }

    #[test]
    fn a_typed_value_is_planned_for_every_changed_cell() {
        let columns = [column("age", "integer", true)];
        let targets = [
            target(0, 0, Value::Int(1)),
            target(1, 0, Value::Int(7)),
            target(2, 0, Value::Int(7)),
        ];
        let plan = plan(&columns, true, SetCellsValue::Text("7".into()), &targets).unwrap();
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(plan.edits[0].value, Value::Int(7));
        assert_eq!(plan.edits[0].row_key, vec![Value::Int(0)]);
        assert_eq!(plan.unchanged, 2);
    }

    #[test]
    fn null_is_planned_for_a_nullable_column() {
        let columns = [column("note", "text", true)];
        let plan = plan(
            &columns,
            true,
            SetCellsValue::Null,
            &[target(0, 0, Value::Text("a".into()))],
        )
        .unwrap();
        assert_eq!(plan.edits[0].value, Value::Null);
    }

    #[test]
    fn null_into_a_not_null_column_is_refused() {
        let columns = [column("note", "text", false)];
        let result = plan(
            &columns,
            true,
            SetCellsValue::Null,
            &[target(0, 0, Value::Text("a".into()))],
        );
        assert_eq!(result, Err(SetCellsError::NotNullable { column: "note".into() }));
    }

    #[test]
    fn text_that_does_not_parse_refuses_the_whole_plan() {
        let columns = [column("age", "integer", true)];
        let targets = [target(0, 0, Value::Int(1)), target(1, 0, Value::Int(2))];
        let result = plan(&columns, true, SetCellsValue::Text("abc".into()), &targets);
        assert!(matches!(result, Err(SetCellsError::InvalidValue { .. })));
    }

    #[test]
    fn key_generated_and_identity_columns_are_refused() {
        for flag in 0..3 {
            let mut info = column("id", "integer", false);
            match flag {
                0 => info.primary_key = true,
                1 => info.is_generated = true,
                _ => info.is_auto_increment = true,
            }
            let result = plan(
                &[info],
                true,
                SetCellsValue::Text("5".into()),
                &[target(0, 0, Value::Int(1))],
            );
            assert_eq!(result, Err(SetCellsError::ReadOnlyColumn { column: "id".into() }));
        }
    }

    #[test]
    fn a_cell_the_grid_cannot_edit_is_refused() {
        let columns = [column("ro_blob", "text", true)];
        let result = plan(
            &columns,
            true,
            SetCellsValue::Text("x".into()),
            &[target(3, 0, Value::Text("a".into()))],
        );
        assert_eq!(
            result,
            Err(SetCellsError::NotEditable {
                column: "ro_blob".into(),
                position: 3
            })
        );
    }

    #[test]
    fn a_table_that_is_not_editable_is_refused() {
        let columns = [column("note", "text", true)];
        let result = plan(&columns, false, SetCellsValue::Null, &[target(0, 0, Value::Null)]);
        assert_eq!(result, Err(SetCellsError::TableNotEditable));
    }

    #[test]
    fn an_empty_or_oversized_selection_is_refused() {
        let columns = [column("note", "text", true)];
        assert_eq!(
            plan(&columns, true, SetCellsValue::Null, &[]),
            Err(SetCellsError::NothingSelected)
        );
        let many: Vec<_> = (0..=MAX_SET_CELLS as u32)
            .map(|row| target(row, 0, Value::Null))
            .collect();
        assert_eq!(
            plan(&columns, true, SetCellsValue::Null, &many),
            Err(SetCellsError::TooManyCells)
        );
    }

    #[test]
    fn rows_without_a_readable_key_are_skipped_not_edited() {
        let columns = [column("note", "text", true)];
        let mut draft = target(0, 0, Value::Text("a".into()));
        draft.row_key = None;
        let mut unreadable = target(1, 0, Value::Text("b".into()));
        unreadable.row_key = Some(vec![Value::Undecodable("NUMERIC".into())]);
        let plan = plan(
            &columns,
            true,
            SetCellsValue::Null,
            &[draft, unreadable, target(2, 0, Value::Text("c".into()))],
        )
        .unwrap();
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(plan.skipped, 2);
    }

    #[test]
    fn planned_edits_are_ordered_by_row_then_column() {
        let columns = [column("a", "text", true), column("b", "text", true)];
        let targets = [
            target(1, 1, Value::Text("x".into())),
            target(0, 1, Value::Text("x".into())),
            target(1, 0, Value::Text("x".into())),
        ];
        let plan = plan(&columns, true, SetCellsValue::Null, &targets).unwrap();
        let order: Vec<_> = plan.edits.iter().map(|edit| (edit.position, edit.column)).collect();
        assert_eq!(order, [(0, 1), (1, 0), (1, 1)]);
    }
}
