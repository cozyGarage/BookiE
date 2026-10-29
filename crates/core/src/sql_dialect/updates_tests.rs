use super::{
    build_keyed_update, build_keyed_value_select, build_mongodb_keyed_delete, build_mongodb_keyed_update,
    build_optimistic_keyed_update,
};
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
        domain_type: None,
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
            Value::Json(Box::new(serde_json::Value::String("id-1".into()))),
            Value::Null,
            Value::Undecodable("missing BSON field".into()),
        ],
        &[Value::Json(Box::new(serde_json::Value::String("id-1".into())))],
    )
    .unwrap();
    assert_eq!(
        sql,
        "DELETE FROM \"appdb\".\"records\" WHERE \"_id\" = ? AND \"nullable\" = ? AND \"missing\" = ? AND tablepro_mongodb_exact_field_set(?)"
    );
    assert_eq!(
        params,
        vec![
            Value::Json(Box::new(serde_json::Value::String("id-1".into()))),
            Value::Null,
            Value::Undecodable("missing BSON field".into()),
            Value::Json(Box::new(serde_json::json!(["_id", "nullable"])))
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
    assert!(matches!(error, crate::sql_dialect::BuildSqlError::UnrepresentableValue { column } if column == "payload"));
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

#[test]
fn optimistic_updates_compare_the_original_value_with_null_safe_dialect_syntax() {
    let mut columns = [column("id", true), column("payload", false)];
    columns[1].nullable = true;
    for (driver, expected) in [
        ("postgres", "\"payload\" IS NOT DISTINCT FROM $3"),
        ("sqlite", "\"payload\" IS NOT DISTINCT FROM ?"),
        ("mysql", "`payload` <=> ?"),
        ("mssql", "([payload] = @P3 OR ([payload] IS NULL AND @P4 IS NULL))"),
    ] {
        let (sql, params) = build_optimistic_keyed_update(
            driver,
            None,
            "records",
            &columns,
            &[(1, Value::Null, Value::Text("edited".into()))],
            &[Value::Int(7)],
        )
        .unwrap();
        assert!(sql.contains(expected), "{driver}: {sql}");
        assert_eq!(params.last(), Some(&Value::Null));
        assert_eq!(params.first(), Some(&Value::Text("edited".into())));
    }
}

#[test]
fn mysql_empty_enum_and_set_keyed_values_use_a_server_empty_string_expression() {
    let mut mood = column("mood", false);
    mood.data_type = "ENUM('', 'ready')".into();
    let mut permissions = column("permissions", false);
    permissions.data_type = "set('read', 'write')".into();
    let columns = [column("id", true), mood, permissions, column("note", false)];
    let (sql, params) = build_keyed_update(
        "mysql",
        None,
        "records",
        &columns,
        &[
            (1, Value::Text(String::new())),
            (2, Value::Text(String::new())),
            (3, Value::Text(String::new())),
        ],
        &[Value::Int(7)],
    )
    .unwrap();

    assert_eq!(
        sql,
        "UPDATE `records` SET `mood` = SPACE(0), `permissions` = SPACE(0), `note` = ? WHERE `id` = ?"
    );
    assert_eq!(params, vec![Value::Text(String::new()), Value::Int(7)]);
}

#[test]
fn keyed_value_select_quotes_target_binds_composite_keys_and_caps_duplicates() {
    let columns = [column("tenant", true), column("id", true), column("payload", false)];
    let (sql, params) = build_keyed_value_select(
        "postgres",
        Some("odd\"schema"),
        "records",
        &columns,
        2,
        &[Value::Text("acme".into()), Value::Int(7)],
    )
    .unwrap();

    assert_eq!(
        sql,
        "SELECT \"payload\" FROM \"odd\"\"schema\".\"records\" WHERE \"tenant\" = $1 AND \"id\" = $2 ORDER BY \"tenant\" ASC, \"id\" ASC LIMIT 2 OFFSET 0"
    );
    assert_eq!(params, vec![Value::Text("acme".into()), Value::Int(7)]);
}

#[test]
fn keyed_value_select_casts_postgres_enum_and_domain_keys() {
    let mut enum_key = column("status", true);
    enum_key.data_type = "status_type".into();
    enum_key.enum_type = Some(crate::QualifiedTypeName {
        schema: "app".into(),
        name: "status_type".into(),
    });
    let mut domain_key = column("domain_status", true);
    domain_key.data_type = "domain_status_type".into();
    domain_key.domain_type = Some(crate::QualifiedTypeName {
        schema: "app".into(),
        name: "domain_status_type".into(),
    });
    let columns = [enum_key, domain_key, column("payload", false)];
    let (sql, params) = build_keyed_value_select(
        "postgres",
        None,
        "records",
        &columns,
        2,
        &[Value::Text("ready".into()), Value::Text("open".into())],
    )
    .unwrap();

    assert_eq!(
        sql,
        "SELECT \"payload\" FROM \"records\" WHERE \"status\" = $1::text::\"app\".\"status_type\" AND \"domain_status\" = $2::text::\"app\".\"domain_status_type\" ORDER BY \"status\" ASC, \"domain_status\" ASC LIMIT 2 OFFSET 0"
    );
    assert_eq!(params, vec![Value::Text("ready".into()), Value::Text("open".into())]);
}

#[test]
fn keyed_value_select_requires_a_primary_key_and_existing_column() {
    let no_key = [column("payload", false)];
    assert!(matches!(
        build_keyed_value_select("postgres", None, "records", &no_key, 0, &[]),
        Err(crate::sql_dialect::BuildSqlError::NoPrimaryKey)
    ));
    let keyed = [column("id", true)];
    assert!(matches!(
        build_keyed_value_select("postgres", None, "records", &keyed, 1, &[Value::Int(1)]),
        Err(crate::sql_dialect::BuildSqlError::StaleColumns)
    ));
}
