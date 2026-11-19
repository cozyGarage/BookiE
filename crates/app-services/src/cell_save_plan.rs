use bookie_core::{ColumnInfo, Value};

use crate::change_tracker::{EditStatement, MaterializeError, RowKey, TabChangeTracker, checked_write_indexes};
use crate::edit_preview::preview_text;
use crate::value_input::{ValueInputError, parse_input_for_column};

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
    #[error("this table has no column named {0}")]
    UnknownColumnName(String),
    #[error("enter a value for at least one column")]
    NothingToInsert,
    #[error("{column}: {reason}")]
    InvalidValue { column: String, reason: String },
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
    finish_plan(&tracker, target)
}

pub fn plan_row_insert_named(
    target: SaveTarget<'_>,
    named: &[(String, String)],
) -> Result<CellSavePlan, CellSaveError> {
    let mut values = Vec::with_capacity(named.len());
    for (name, text) in named {
        let at = target
            .columns
            .iter()
            .position(|column| &column.name == name)
            .ok_or_else(|| CellSaveError::UnknownColumnName(name.clone()))?;
        values.push((at, text.clone()));
    }
    plan_row_insert(target, &values)
}

pub fn plan_row_insert(target: SaveTarget<'_>, values: &[(usize, String)]) -> Result<CellSavePlan, CellSaveError> {
    let mut tracker = TabChangeTracker::new();
    let draft = tracker.track_insert(vec![Value::Null; target.columns.len()]);
    let RowKey::Draft(draft_id) = draft else {
        return Err(CellSaveError::Build("a new row could not be started".into()));
    };
    let mut touched = 0;
    for (column, text) in values {
        if text.is_empty() {
            continue;
        }
        let info = target
            .columns
            .get(*column)
            .ok_or(CellSaveError::UnknownColumn(*column))?;
        let value = parse_input_for_column(text, Some(info)).map_err(|error| CellSaveError::InvalidValue {
            column: info.name.clone(),
            reason: readable(&error),
        })?;
        tracker.track_draft_cell_edit(draft_id, *column, value);
        touched += 1;
    }
    if touched == 0 {
        return Err(CellSaveError::NothingToInsert);
    }
    finish_plan(&tracker, target)
}

fn readable(error: &ValueInputError) -> String {
    let name = format!("{error:?}");
    let name = name.split(['(', '{', ' ']).next().unwrap_or("");
    let mut words = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            words.push(' ');
        }
        words.extend(character.to_lowercase());
    }
    words
}

pub fn plan_row_deletes(
    target: SaveTarget<'_>,
    grid_columns: &[String],
    rows: &[Vec<Value>],
) -> Result<CellSavePlan, CellSaveError> {
    let key_positions = key_positions(target.columns, grid_columns)?;
    let mut tracker = TabChangeTracker::new();
    for row in rows {
        let key_values: Vec<Value> = key_positions
            .iter()
            .map(|position| row.get(*position).cloned().unwrap_or(Value::Null))
            .collect();
        let row_key = RowKey::from_pk_values(&key_values).ok_or(CellSaveError::UnreadableKey)?;
        let original: Vec<Value> = target
            .columns
            .iter()
            .map(|column| {
                grid_columns
                    .iter()
                    .position(|name| name == &column.name)
                    .and_then(|position| row.get(position).cloned())
                    .unwrap_or(Value::Null)
            })
            .collect();
        tracker.track_delete(row_key, original);
    }
    finish_plan(&tracker, target)
}

fn finish_plan(tracker: &TabChangeTracker, target: SaveTarget<'_>) -> Result<CellSavePlan, CellSaveError> {
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

    #[test]
    fn selected_rows_become_keyed_deletes_that_must_each_touch_one_row() {
        let columns = orders();
        let plan = plan_row_deletes(target(&columns), &names(), &[row(1, "ada", 1.5), row(3, "cy", 3.5)]).unwrap();

        assert_eq!(plan.statements.len(), 2);
        for (sql, _) in &plan.statements {
            assert!(sql.starts_with("DELETE"), "{sql}");
        }
        let keys: Vec<&Value> = plan
            .statements
            .iter()
            .filter_map(|(_, values)| values.first())
            .collect();
        assert!(
            keys.contains(&&Value::Int(1)) && keys.contains(&&Value::Int(3)),
            "{keys:?}"
        );
        assert_eq!(plan.expect_one, vec![0, 1]);
        assert!(plan.preview.contains("DELETE"), "{}", plan.preview);
    }

    #[test]
    fn the_same_row_selected_twice_is_deleted_once_and_a_keyless_table_cannot_delete() {
        let columns = orders();
        let twice = plan_row_deletes(target(&columns), &names(), &[row(2, "bob", 2.5), row(2, "bob", 2.5)]).unwrap();
        assert_eq!(twice.statements.len(), 1);

        let keyless = vec![column("customer", false), column("total", false)];
        let result = plan_row_deletes(
            target(&keyless),
            &["customer".to_string(), "total".to_string()],
            &[vec![Value::Text("ada".into()), Value::Float(1.5)]],
        );
        assert_eq!(result, Err(CellSaveError::NoPrimaryKey));
    }

    fn insertable() -> Vec<ColumnInfo> {
        let mut id = column("id", true);
        id.data_type = "INTEGER".into();
        id.nullable = false;
        id.is_auto_increment = true;
        let mut customer = column("customer", false);
        customer.data_type = "TEXT".into();
        let mut total = column("total", false);
        total.data_type = "REAL".into();
        vec![id, customer, total]
    }

    #[test]
    fn a_new_row_inserts_only_the_columns_that_were_filled_in() {
        let columns = insertable();
        let plan = plan_row_insert(
            target(&columns),
            &[(0, String::new()), (1, "zed".into()), (2, "4.5".into())],
        )
        .unwrap();

        assert_eq!(plan.statements.len(), 1);
        let (sql, values) = &plan.statements[0];
        assert!(sql.starts_with("INSERT"), "{sql}");
        assert!(
            !sql.contains("\"id\""),
            "the empty key column is left to the database: {sql}"
        );
        assert_eq!(values, &vec![Value::Text("zed".into()), Value::Float(4.5)]);
        assert!(plan.expect_one.is_empty(), "an insert has no existing row to check");
    }

    #[test]
    fn a_value_that_does_not_fit_its_column_names_the_column_and_nothing_is_planned() {
        let columns = insertable();
        let result = plan_row_insert(target(&columns), &[(2, "abc".into())]);
        match result {
            Err(CellSaveError::InvalidValue { column, reason }) => {
                assert_eq!(column, "total");
                assert!(!reason.is_empty());
            }
            other => panic!("expected an invalid value, got {other:?}"),
        }
    }

    #[test]
    fn a_row_with_no_values_is_refused() {
        let columns = insertable();
        let result = plan_row_insert(target(&columns), &[(1, String::new()), (2, String::new())]);
        assert_eq!(result, Err(CellSaveError::NothingToInsert));
    }

    #[test]
    fn a_field_for_a_column_the_table_no_longer_has_is_an_error_not_a_dropped_value() {
        let columns = insertable();
        let result = plan_row_insert_named(
            target(&columns),
            &[("customer".into(), "zed".into()), ("gone".into(), "x".into())],
        );
        assert_eq!(result, Err(CellSaveError::UnknownColumnName("gone".into())));

        let named = plan_row_insert_named(target(&columns), &[("total".into(), "2".into())]).unwrap();
        assert_eq!(named.statements.len(), 1);
    }

    #[test]
    fn an_empty_field_leaves_a_column_with_a_default_out_of_the_statement() {
        let mut columns = insertable();
        columns[1].default_value = Some("'anonymous'".into());
        let plan = plan_row_insert(target(&columns), &[(1, String::new()), (2, "9.5".into())]).unwrap();
        let (sql, values) = &plan.statements[0];
        assert!(
            !sql.contains("\"customer\""),
            "the column with a default is left to the database: {sql}"
        );
        assert_eq!(values, &vec![Value::Float(9.5)]);
    }
}
