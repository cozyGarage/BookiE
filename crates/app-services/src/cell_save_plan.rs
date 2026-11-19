use bookie_core::{ColumnInfo, Value};

use crate::change_tracker::{EditStatement, MaterializeError, RowKey, TabChangeTracker, checked_write_indexes};
use crate::edit_preview::preview_text;

#[derive(Debug, Clone, Copy)]
pub struct SaveTarget<'a> {
    pub driver_id: &'a str,
    pub schema: Option<&'a str>,
    pub table: &'a str,
    pub columns: &'a [ColumnInfo],
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellChange {
    pub original_row: Vec<Value>,
    pub column: usize,
    pub new_value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellSavePlan {
    pub statements: Vec<EditStatement>,
    pub expect_one: Vec<usize>,
    pub preview: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CellSaveError {
    #[error("this table has no primary key, so a row cannot be identified for saving")]
    NoPrimaryKey,
    #[error("a row's key value could not be read, so that row cannot be saved")]
    UnreadableKey,
    #[error("column {0} is not a column of this table")]
    UnknownColumn(usize),
    #[error("{0} is part of the primary key, which identifies the row and cannot be edited here")]
    KeyColumnEdit(String),
    #[error("{0}")]
    Build(String),
}

pub fn plan_cell_saves(
    target: SaveTarget<'_>,
    grid_columns: &[String],
    changes: &[CellChange],
) -> Result<CellSavePlan, CellSaveError> {
    let key_positions = key_positions(target.columns, grid_columns)?;
    let mut tracker = TabChangeTracker::new();
    for change in changes {
        let column_name = grid_columns
            .get(change.column)
            .ok_or(CellSaveError::UnknownColumn(change.column))?;
        let table_column = target
            .columns
            .iter()
            .position(|column| &column.name == column_name)
            .ok_or(CellSaveError::UnknownColumn(change.column))?;
        if target.columns[table_column].primary_key {
            return Err(CellSaveError::KeyColumnEdit(column_name.clone()));
        }
        let key_values: Vec<Value> = key_positions
            .iter()
            .map(|position| change.original_row.get(*position).cloned().unwrap_or(Value::Null))
            .collect();
        let row_key = RowKey::from_pk_values(&key_values).ok_or(CellSaveError::UnreadableKey)?;
        let original = change.original_row.get(change.column).cloned().unwrap_or(Value::Null);
        if original != change.new_value {
            tracker.track_cell_edit(row_key, table_column, original, change.new_value.clone());
        }
    }
    let (statements, sources) = tracker
        .materialize(target.driver_id, target.schema, target.table, target.columns)
        .map_err(|error| match error {
            MaterializeError::UnreadableRowKey => CellSaveError::UnreadableKey,
            other => CellSaveError::Build(other.to_string()),
        })?;
    let expect_one = checked_write_indexes(Some(target.driver_id), &sources);
    let preview = preview_text(target.driver_id, &statements);
    Ok(CellSavePlan {
        statements,
        expect_one,
        preview,
    })
}

fn key_positions(columns: &[ColumnInfo], grid_columns: &[String]) -> Result<Vec<usize>, CellSaveError> {
    let positions: Vec<usize> = columns
        .iter()
        .filter(|column| column.primary_key)
        .filter_map(|key| grid_columns.iter().position(|name| name == &key.name))
        .collect();
    let key_count = columns.iter().filter(|column| column.primary_key).count();
    if key_count == 0 {
        return Err(CellSaveError::NoPrimaryKey);
    }
    if positions.len() != key_count {
        return Err(CellSaveError::UnreadableKey);
    }
    Ok(positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, primary_key: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "TEXT".into(),
            nullable: true,
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

    fn orders() -> Vec<ColumnInfo> {
        vec![column("id", true), column("customer", false), column("total", false)]
    }

    fn names() -> Vec<String> {
        ["id", "customer", "total"].map(String::from).to_vec()
    }

    fn row(id: i64, customer: &str, total: f64) -> Vec<Value> {
        vec![Value::Int(id), Value::Text(customer.into()), Value::Float(total)]
    }

    fn target(columns: &[ColumnInfo]) -> SaveTarget<'_> {
        SaveTarget {
            driver_id: "sqlite",
            schema: None,
            table: "orders",
            columns,
        }
    }

    #[test]
    fn an_edited_cell_becomes_one_keyed_update_that_must_touch_one_row() {
        let columns = orders();
        let plan = plan_cell_saves(
            target(&columns),
            &names(),
            &[CellChange {
                original_row: row(2, "bob", 2.5),
                column: 1,
                new_value: Value::Text("robert".into()),
            }],
        )
        .unwrap();

        assert_eq!(plan.statements.len(), 1);
        let (sql, values) = &plan.statements[0];
        assert!(sql.starts_with("UPDATE"), "{sql}");
        assert!(values.contains(&Value::Text("robert".into())));
        assert!(
            values.contains(&Value::Int(2)),
            "the key identifies the row: {values:?}"
        );
        assert_eq!(plan.expect_one, vec![0]);
        assert!(plan.preview.contains("UPDATE"), "{}", plan.preview);
    }

    #[test]
    fn two_edits_in_one_row_make_one_update_and_two_rows_make_two() {
        let columns = orders();
        let same_row = plan_cell_saves(
            target(&columns),
            &names(),
            &[
                CellChange {
                    original_row: row(1, "ada", 1.5),
                    column: 1,
                    new_value: Value::Text("ada l".into()),
                },
                CellChange {
                    original_row: row(1, "ada", 1.5),
                    column: 2,
                    new_value: Value::Float(9.0),
                },
            ],
        )
        .unwrap();
        assert_eq!(same_row.statements.len(), 1);

        let two_rows = plan_cell_saves(
            target(&columns),
            &names(),
            &[
                CellChange {
                    original_row: row(1, "ada", 1.5),
                    column: 1,
                    new_value: Value::Text("x".into()),
                },
                CellChange {
                    original_row: row(2, "bob", 2.5),
                    column: 1,
                    new_value: Value::Text("y".into()),
                },
            ],
        )
        .unwrap();
        assert_eq!(two_rows.statements.len(), 2);
        assert_eq!(two_rows.expect_one, vec![0, 1]);
    }

    #[test]
    fn a_value_set_back_to_what_it_was_makes_no_statement() {
        let columns = orders();
        let plan = plan_cell_saves(
            target(&columns),
            &names(),
            &[CellChange {
                original_row: row(1, "ada", 1.5),
                column: 1,
                new_value: Value::Text("ada".into()),
            }],
        )
        .unwrap();
        assert!(plan.statements.is_empty());
    }

    #[test]
    fn a_table_without_a_primary_key_cannot_be_saved() {
        let columns = vec![column("customer", false), column("total", false)];
        let result = plan_cell_saves(
            target(&columns),
            &["customer".to_string(), "total".to_string()],
            &[CellChange {
                original_row: vec![Value::Text("ada".into()), Value::Float(1.5)],
                column: 0,
                new_value: Value::Text("x".into()),
            }],
        );
        assert_eq!(result, Err(CellSaveError::NoPrimaryKey));
    }

    #[test]
    fn a_primary_key_column_cannot_be_edited() {
        let columns = orders();
        let result = plan_cell_saves(
            target(&columns),
            &names(),
            &[CellChange {
                original_row: row(1, "ada", 1.5),
                column: 0,
                new_value: Value::Int(99),
            }],
        );
        assert_eq!(result, Err(CellSaveError::KeyColumnEdit("id".into())));
    }

    #[test]
    fn a_key_column_that_the_grid_does_not_show_cannot_be_saved() {
        let columns = orders();
        let result = plan_cell_saves(
            target(&columns),
            &["customer".to_string(), "total".to_string()],
            &[CellChange {
                original_row: vec![Value::Text("ada".into()), Value::Float(1.5)],
                column: 0,
                new_value: Value::Text("x".into()),
            }],
        );
        assert_eq!(result, Err(CellSaveError::UnreadableKey));
    }
}
