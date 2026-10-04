use super::{
    BuildSqlError, build_update, checked_pk_indexes, keyed_where_clause, placeholder_for, postgres_text_cast_type,
    qualified_table, quote_ident,
};
use crate::{ColumnInfo, Value};

pub fn build_keyed_update(
    driver_id: &str,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    edits: &[(usize, Value)],
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    if edits.is_empty() {
        return Err(BuildSqlError::NothingToUpdate);
    }
    if edits.iter().any(|(col_idx, _)| *col_idx >= columns.len()) {
        return Err(BuildSqlError::StaleColumns);
    }
    let mut params: Vec<Value> = Vec::with_capacity(edits.len() + pk_values.len());
    let set_clauses: Vec<String> = edits
        .iter()
        .map(|(col_idx, new_value)| {
            let placeholder = placeholder_for(driver_id, params.len());
            let value_sql = if driver_id == "postgres" {
                postgres_text_cast_type(&columns[*col_idx], new_value)
                    .map(|type_name| format!("{placeholder}::text::{type_name}"))
                    .unwrap_or(placeholder)
            } else {
                placeholder
            };
            let clause = format!("{} = {}", quote_ident(driver_id, &columns[*col_idx].name), value_sql);
            params.push(new_value.clone());
            clause
        })
        .collect();
    let where_clause = keyed_where_clause(driver_id, columns, &pk_indexes, pk_values, &mut params);
    let qualified = qualified_table(driver_id, schema, table);
    let sql = build_update(driver_id, &qualified, &set_clauses.join(", "), &where_clause);
    Ok((sql, params))
}

/// Build a MongoDB grid update with compare-and-set predicates for every
/// edited field. MongoDB's update adapter translates equality against BSON
/// NULL to an explicit-null type check, keeping missing fields distinct.
pub fn build_mongodb_keyed_update(
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    edits: &[(usize, Value, Value)],
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let driver_id = "mongodb";
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    if edits.is_empty() {
        return Err(BuildSqlError::NothingToUpdate);
    }
    if edits.iter().any(|(col_idx, _, _)| *col_idx >= columns.len()) {
        return Err(BuildSqlError::StaleColumns);
    }

    let mut params = Vec::with_capacity(edits.len() * 2 + pk_values.len());
    let set_clauses = edits
        .iter()
        .map(|(col_idx, _, new_value)| {
            let placeholder = placeholder_for(driver_id, params.len());
            params.push(new_value.clone());
            format!("{} = {placeholder}", quote_ident(driver_id, &columns[*col_idx].name))
        })
        .collect::<Vec<_>>();
    let mut where_clauses = vec![keyed_where_clause(
        driver_id,
        columns,
        &pk_indexes,
        pk_values,
        &mut params,
    )];
    for (col_idx, original_value, _) in edits {
        if columns[*col_idx].primary_key {
            continue;
        }
        let placeholder = placeholder_for(driver_id, params.len());
        where_clauses.push(format!(
            "{} = {placeholder}",
            quote_ident(driver_id, &columns[*col_idx].name)
        ));
        params.push(original_value.clone());
    }
    let qualified = qualified_table(driver_id, schema, table);
    let sql = build_update(
        driver_id,
        &qualified,
        &set_clauses.join(", "),
        &where_clauses.join(" AND "),
    );
    Ok((sql, params))
}

pub fn build_mongodb_keyed_delete(
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    original_values: &[Value],
    pk_values: &[Value],
) -> Result<(String, Vec<Value>), BuildSqlError> {
    let driver_id = "mongodb";
    if original_values.len() != columns.len() {
        return Err(BuildSqlError::LengthMismatch {
            expected: columns.len(),
            got: original_values.len(),
        });
    }
    let pk_indexes = checked_pk_indexes(columns, pk_values)?;
    let mut params = Vec::with_capacity(pk_values.len() + columns.len() - pk_indexes.len());
    let mut where_clauses = vec![keyed_where_clause(
        driver_id,
        columns,
        &pk_indexes,
        pk_values,
        &mut params,
    )];
    for (column_index, (column, original_value)) in columns.iter().zip(original_values).enumerate() {
        if pk_indexes.contains(&column_index) {
            continue;
        }
        if matches!(original_value, Value::Undecodable(kind) if kind != "missing BSON field") {
            return Err(BuildSqlError::UnrepresentableValue {
                column: column.name.clone(),
            });
        }
        let placeholder = placeholder_for(driver_id, params.len());
        where_clauses.push(format!("{} = {placeholder}", quote_ident(driver_id, &column.name)));
        params.push(original_value.clone());
    }
    let qualified = qualified_table(driver_id, schema, table);
    Ok((
        format!("DELETE FROM {qualified} WHERE {}", where_clauses.join(" AND ")),
        params,
    ))
}

#[cfg(test)]
mod tests {
    use super::{build_keyed_update, build_mongodb_keyed_delete, build_mongodb_keyed_update};
    use crate::{ColumnInfo, Value};

    fn column(name: &str, primary_key: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: !primary_key,
            primary_key,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
        }
    }

    #[test]
    fn mongodb_update_guards_edited_fields_with_original_values() {
        let columns = [column("_id", true), column("payload", false)];
        let (sql, params) = build_mongodb_keyed_update(
            Some("appdb"),
            "records",
            &columns,
            &[(1, Value::Text("before".into()), Value::Text("edited".into()))],
            &[Value::Text("id-1".into())],
        )
        .unwrap();
        assert_eq!(
            sql,
            "UPDATE \"appdb\".\"records\" SET \"payload\" = ? WHERE \"_id\" = ? AND \"payload\" = ?"
        );
        assert_eq!(
            params,
            vec![
                Value::Text("edited".into()),
                Value::Text("id-1".into()),
                Value::Text("before".into())
            ]
        );
    }

    #[test]
    fn mongodb_update_keeps_null_original_value_as_guard_parameter() {
        let columns = [column("_id", true), column("payload", false)];
        let (_, params) = build_mongodb_keyed_update(
            Some("appdb"),
            "records",
            &columns,
            &[(1, Value::Null, Value::Text("edited".into()))],
            &[Value::Text("id-1".into())],
        )
        .unwrap();
        assert_eq!(
            params,
            vec![Value::Text("edited".into()), Value::Text("id-1".into()), Value::Null]
        );
    }

    #[test]
    fn mongodb_delete_guards_every_non_key_value_including_null_and_missing() {
        let columns = [column("_id", true), column("nullable", false), column("missing", false)];
        let (sql, params) = build_mongodb_keyed_delete(
            Some("appdb"),
            "records",
            &columns,
            &[
                Value::Json(serde_json::Value::String("id-1".into())),
                Value::Null,
                Value::Undecodable("missing BSON field".into()),
            ],
            &[Value::Json(serde_json::Value::String("id-1".into()))],
        )
        .unwrap();
        assert_eq!(
            sql,
            "DELETE FROM \"appdb\".\"records\" WHERE \"_id\" = ? AND \"nullable\" = ? AND \"missing\" = ?"
        );
        assert_eq!(
            params,
            vec![
                Value::Json(serde_json::Value::String("id-1".into())),
                Value::Null,
                Value::Undecodable("missing BSON field".into())
            ]
        );
    }

    #[test]
    fn mongodb_delete_refuses_other_undecodable_snapshot_values() {
        let columns = [column("_id", true), column("payload", false)];
        let error = build_mongodb_keyed_delete(
            None,
            "records",
            &columns,
            &[Value::Int(1), Value::Undecodable("NUMERIC".into())],
            &[Value::Int(1)],
        )
        .unwrap_err();
        assert!(
            matches!(error, crate::sql_dialect::BuildSqlError::UnrepresentableValue { column } if column == "payload")
        );
    }

    #[test]
    fn ordinary_keyed_updates_keep_the_existing_primary_key_filter() {
        let columns = [column("id", true), column("payload", false)];
        let (sql, params) = build_keyed_update(
            "postgres",
            None,
            "records",
            &columns,
            &[(1, Value::Text("edited".into()))],
            &[Value::Int(7)],
        )
        .unwrap();
        assert_eq!(sql, "UPDATE \"records\" SET \"payload\" = $1 WHERE \"id\" = $2");
        assert_eq!(params, vec![Value::Text("edited".into()), Value::Int(7)]);
    }
}
