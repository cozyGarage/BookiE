use super::*;

fn col(name: &str, pk: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: false,
        primary_key: pk,
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
fn validate_ident_accepts_ordinary_and_quoted_names() {
    for name in ["users", "Order Details", "naïve", "a\"b", "a`b", "a]b"] {
        assert_eq!(validate_ident(name), Ok(()), "{name}");
    }
}

#[test]
fn validate_ident_rejects_empty_overlong_and_control_characters() {
    assert_eq!(validate_ident(""), Err(IdentError::Empty));
    assert_eq!(validate_ident("   "), Err(IdentError::Empty));
    assert_eq!(
        validate_ident(&"a".repeat(MAX_IDENT_BYTES + 1)),
        Err(IdentError::TooLong)
    );
    for name in ["a\0b", "a\nb", "a\rb", "a\u{2028}b", "a\u{7f}b"] {
        assert_eq!(validate_ident(name), Err(IdentError::ControlCharacter), "{name:?}");
    }
}

#[test]
fn quote_ident_dialect() {
    assert_eq!(quote_ident("postgres", "users"), "\"users\"");
    assert_eq!(quote_ident("sqlite", "users"), "\"users\"");
    assert_eq!(quote_ident("mysql", "users"), "`users`");
    assert_eq!(quote_ident("clickhouse", "users"), "`users`");
    assert_eq!(quote_ident("clickhouse", "a`b"), "`a``b`");
}

#[test]
fn quote_ident_doubles_embedded_delimiter() {
    assert_eq!(quote_ident("postgres", "foo\"bar"), "\"foo\"\"bar\"");
    assert_eq!(quote_ident("mysql", "foo`bar"), "`foo``bar`");
}

/// The exact shape that broke identifier quoting: a trailing backslash
/// consumes the closing backtick, so text after it parses as SQL rather
/// than staying part of the identifier. Mirrors the literal-escaping
/// regression test in sql_literal.rs.
#[test]
fn clickhouse_identifier_quoting_escapes_a_trailing_backslash() {
    assert_eq!(quote_ident("clickhouse", "evil\\"), "`evil\\\\`");
    assert_eq!(
        quote_ident("clickhouse", "evil\\`; DROP TABLE users; --"),
        "`evil\\\\``; DROP TABLE users; --`"
    );
}

#[test]
fn mysql_identifier_quoting_does_not_touch_a_backslash() {
    // MySQL identifiers (unlike its string literals) do not treat
    // backslash as an escape character, so doubling it here would
    // corrupt the identifier instead of protecting it.
    assert_eq!(quote_ident("mysql", "evil\\"), "`evil\\`");
}

#[test]
fn quote_ident_keeps_a_control_character_so_the_statement_names_the_same_object() {
    assert_eq!(quote_ident("postgres", "a\nb"), "\"a\nb\"");
}

#[test]
fn quote_ident_mssql() {
    assert_eq!(quote_ident("mssql", "users"), "[users]");
    assert_eq!(quote_ident("mssql", "a]b"), "[a]]b]");
}

#[test]
fn placeholder_dialect() {
    assert_eq!(placeholder_for("postgres", 0), "$1");
    assert_eq!(placeholder_for("postgres", 2), "$3");
    assert_eq!(placeholder_for("sqlite", 0), "?");
    assert_eq!(placeholder_for("mysql", 5), "?");
}

#[test]
fn placeholder_mssql() {
    assert_eq!(placeholder_for("mssql", 0), "@P1");
    assert_eq!(placeholder_for("mssql", 2), "@P3");
}

#[test]
fn pagination_limit_offset_dialects() {
    assert_eq!(
        build_order_and_pagination("postgres", None, 50, 100),
        " LIMIT 50 OFFSET 100"
    );
    assert_eq!(
        build_order_and_pagination("mysql", Some("`a` ASC"), 50, 100),
        " ORDER BY `a` ASC LIMIT 50 OFFSET 100"
    );
    assert_eq!(
        build_order_and_pagination("sqlite", Some("\"a\" DESC"), 10, 0),
        " ORDER BY \"a\" DESC LIMIT 10 OFFSET 0"
    );
}

#[test]
fn pagination_mssql_uses_offset_fetch() {
    assert_eq!(
        build_order_and_pagination("mssql", Some("[a] ASC"), 50, 100),
        " ORDER BY [a] ASC OFFSET 100 ROWS FETCH NEXT 50 ROWS ONLY"
    );
}

#[test]
fn pagination_mssql_synthesizes_order_by_when_unsorted() {
    // OFFSET / FETCH is a suffix of ORDER BY in T-SQL, so an
    // unsorted page still needs one to parse at all.
    let sql = build_order_and_pagination("mssql", None, 50, 0);
    assert_eq!(sql, " ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 50 ROWS ONLY");
}

#[test]
fn pagination_treats_blank_order_by_as_absent() {
    assert_eq!(
        build_order_and_pagination("postgres", Some("  "), 5, 0),
        " LIMIT 5 OFFSET 0"
    );
    assert_eq!(
        build_order_and_pagination("mssql", Some("  "), 5, 0),
        " ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 5 ROWS ONLY"
    );
}

#[test]
fn single_cell_update_postgres() {
    let columns = vec![col("id", true), col("name", false)];
    let original = vec![Value::Int(7), Value::Text("alice".into())];
    let (sql, params) =
        build_single_cell_update("postgres", "u", &columns, &original, 1, Value::Text("bob".into())).unwrap();
    assert_eq!(sql, "UPDATE \"u\" SET \"name\" = $1 WHERE \"id\" = $2");
    assert_eq!(params, vec![Value::Text("bob".into()), Value::Int(7)]);
}

#[test]
fn single_cell_update_mysql() {
    let columns = vec![col("id", true), col("name", false)];
    let original = vec![Value::Int(7), Value::Text("alice".into())];
    let (sql, params) =
        build_single_cell_update("mysql", "u", &columns, &original, 1, Value::Text("bob".into())).unwrap();
    assert_eq!(sql, "UPDATE `u` SET `name` = ? WHERE `id` = ?");
    assert_eq!(params, vec![Value::Text("bob".into()), Value::Int(7)]);
}

#[test]
fn single_cell_update_clickhouse() {
    let columns = vec![col("id", true), col("name", false)];
    let original = vec![Value::Int(7), Value::Text("alice".into())];
    let (sql, params) =
        build_single_cell_update("clickhouse", "u", &columns, &original, 1, Value::Text("bob".into())).unwrap();
    assert_eq!(sql, "ALTER TABLE `u` UPDATE `name` = ? WHERE `id` = ?");
    assert_eq!(params, vec![Value::Text("bob".into()), Value::Int(7)]);
}

#[test]
fn full_row_update_clickhouse() {
    let columns = vec![col("id", true), col("a", false), col("b", false)];
    let original = vec![Value::Int(1), Value::Text("x".into()), Value::Text("y".into())];
    let new_values = vec![Value::Int(1), Value::Text("x2".into()), Value::Text("y2".into())];
    let (sql, _) = build_full_row_update("clickhouse", "t", &columns, &original, &new_values).unwrap();
    assert_eq!(sql, "ALTER TABLE `t` UPDATE `a` = ?, `b` = ? WHERE `id` = ?");
}

#[test]
fn build_update_keeps_standard_syntax_for_other_dialects() {
    assert_eq!(
        build_update("postgres", "\"t\"", "\"a\" = $1", "\"id\" = $2"),
        "UPDATE \"t\" SET \"a\" = $1 WHERE \"id\" = $2"
    );
    assert_eq!(
        build_update("clickhouse", "`t`", "`a` = ?", "`id` = ?"),
        "ALTER TABLE `t` UPDATE `a` = ? WHERE `id` = ?"
    );
}

#[test]
fn single_cell_update_sqlite() {
    let columns = vec![col("id", true), col("v", false)];
    let original = vec![Value::Int(1), Value::Text("a".into())];
    let (sql, _) = build_single_cell_update("sqlite", "t", &columns, &original, 1, Value::Text("b".into())).unwrap();
    assert_eq!(sql, "UPDATE \"t\" SET \"v\" = ? WHERE \"id\" = ?");
}

#[test]
fn single_cell_update_mssql() {
    let columns = vec![col("id", true), col("name", false)];
    let original = vec![Value::Int(7), Value::Text("alice".into())];
    let (sql, params) =
        build_single_cell_update("mssql", "u", &columns, &original, 1, Value::Text("bob".into())).unwrap();
    assert_eq!(sql, "UPDATE [u] SET [name] = @P1 WHERE [id] = @P2");
    assert_eq!(params, vec![Value::Text("bob".into()), Value::Int(7)]);
}

#[test]
fn single_cell_update_no_pk() {
    let columns = vec![col("a", false), col("b", false)];
    let original = vec![Value::Int(1), Value::Int(2)];
    let err = build_single_cell_update("sqlite", "t", &columns, &original, 0, Value::Int(9)).unwrap_err();
    assert!(matches!(err, BuildSqlError::NoPrimaryKey));
}

#[test]
fn single_cell_update_composite_pk() {
    let columns = vec![col("a", true), col("b", true), col("c", false)];
    let original = vec![Value::Int(1), Value::Int(2), Value::Text("x".into())];
    let (sql, params) =
        build_single_cell_update("postgres", "t", &columns, &original, 2, Value::Text("y".into())).unwrap();
    assert_eq!(sql, "UPDATE \"t\" SET \"c\" = $1 WHERE \"a\" = $2 AND \"b\" = $3");
    assert_eq!(params.len(), 3);
}

#[test]
fn full_row_update_skips_pk() {
    let columns = vec![col("id", true), col("name", false), col("age", false)];
    let original = vec![Value::Int(3), Value::Text("a".into()), Value::Int(20)];
    let new_values = vec![Value::Int(3), Value::Text("b".into()), Value::Int(21)];
    let (sql, params) = build_full_row_update("mysql", "p", &columns, &original, &new_values).unwrap();
    assert_eq!(sql, "UPDATE `p` SET `name` = ?, `age` = ? WHERE `id` = ?");
    assert_eq!(params.len(), 3);
    assert_eq!(params[2], Value::Int(3));
}

#[test]
fn full_row_update_length_mismatch() {
    let columns = vec![col("id", true), col("v", false)];
    let original = vec![Value::Int(1), Value::Int(2)];
    let new_values = vec![Value::Int(1)];
    let err = build_full_row_update("postgres", "t", &columns, &original, &new_values).unwrap_err();
    assert!(matches!(err, BuildSqlError::LengthMismatch { expected: 2, got: 1 }));
}

fn col_auto(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "integer".into(),
        nullable: false,
        primary_key: true,
        is_auto_increment: true,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn col_with_default(name: &str, default: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "timestamp".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: Some(default.into()),
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn col_generated(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "integer".into(),
        nullable: false,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: true,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[test]
fn insert_from_draft_skips_auto_increment_pk() {
    let columns = vec![col_auto("id"), col("name", false)];
    let values = vec![Value::Null, Value::Text("alice".into())];
    let (sql, params) = build_insert_from_draft("postgres", None, "users", &columns, &values).unwrap();
    assert_eq!(sql, "INSERT INTO \"users\" (\"name\") VALUES ($1)");
    assert_eq!(params, vec![Value::Text("alice".into())]);
}

#[test]
fn value_contract_postgres_temporal_insert_cast_is_limited_to_postgres_text_values() {
    let mut date = col("value", false);
    date.data_type = "date".into();
    let (sql, _) = build_insert_from_draft(
        "postgres",
        None,
        "dates",
        std::slice::from_ref(&date),
        &[Value::Text("-infinity".into())],
    )
    .unwrap();
    assert_eq!(
        sql,
        "INSERT INTO \"dates\" (\"value\") VALUES ($1::text::pg_catalog.date)"
    );

    let native_date = Value::Date(chrono::NaiveDate::from_ymd_opt(2001, 2, 3).unwrap());
    let (sql, _) =
        build_insert_from_draft("postgres", None, "dates", std::slice::from_ref(&date), &[native_date]).unwrap();
    assert_eq!(sql, "INSERT INTO \"dates\" (\"value\") VALUES ($1)");

    let (sql, _) =
        build_insert_from_draft("mysql", None, "dates", &[date], &[Value::Text("-infinity".into())]).unwrap();
    assert_eq!(sql, "INSERT INTO `dates` (`value`) VALUES (?)");
}

#[test]
fn insert_from_draft_skips_generated_columns() {
    let columns = vec![col("a", false), col_generated("total"), col("b", false)];
    let values = vec![Value::Int(1), Value::Int(99), Value::Int(2)];
    let (sql, params) = build_insert_from_draft("mysql", None, "t", &columns, &values).unwrap();
    assert_eq!(sql, "INSERT INTO `t` (`a`, `b`) VALUES (?, ?)");
    assert_eq!(params, vec![Value::Int(1), Value::Int(2)]);
}

#[test]
fn insert_from_draft_mssql() {
    let columns = vec![col_auto("id"), col("name", false)];
    let values = vec![Value::Null, Value::Text("alice".into())];
    let (sql, params) = build_insert_from_draft("mssql", None, "users", &columns, &values).unwrap();
    assert_eq!(sql, "INSERT INTO [users] ([name]) VALUES (@P1)");
    assert_eq!(params, vec![Value::Text("alice".into())]);
}

#[test]
fn insert_from_draft_omits_null_when_default_exists() {
    // Cell is NULL and column has a server default (e.g., now()) →
    // omit the column from INSERT so the server applies its default.
    let columns = vec![col("name", false), col_with_default("created_at", "now()")];
    let values = vec![Value::Text("bob".into()), Value::Null];
    let (sql, _) = build_insert_from_draft("postgres", None, "u", &columns, &values).unwrap();
    assert_eq!(sql, "INSERT INTO \"u\" (\"name\") VALUES ($1)");
}

#[test]
fn insert_from_draft_keeps_explicit_null_without_default() {
    let columns = vec![col("name", false), col("nickname", false)];
    let values = vec![Value::Text("bob".into()), Value::Null];
    let (sql, params) = build_insert_from_draft("postgres", None, "u", &columns, &values).unwrap();
    assert_eq!(sql, "INSERT INTO \"u\" (\"name\", \"nickname\") VALUES ($1, $2)");
    assert_eq!(params, vec![Value::Text("bob".into()), Value::Null]);
}

#[test]
fn insert_from_draft_qualifies_with_schema() {
    let columns = vec![col("id", true), col("name", false)];
    let values = vec![Value::Int(1), Value::Text("a".into())];
    let (sql, _) = build_insert_from_draft("postgres", Some("public"), "u", &columns, &values).unwrap();
    assert_eq!(sql, "INSERT INTO \"public\".\"u\" (\"id\", \"name\") VALUES ($1, $2)");
}

#[test]
fn where_clause_uses_is_null_for_null_pk_components() {
    let columns = vec![col("a", true), col("b", true), col("c", false)];
    let original = vec![Value::Int(1), Value::Null, Value::Text("x".into())];
    let (sql, params) =
        build_single_cell_update("postgres", "t", &columns, &original, 2, Value::Text("y".into())).unwrap();
    assert_eq!(sql, "UPDATE \"t\" SET \"c\" = $1 WHERE \"a\" = $2 AND \"b\" IS NULL");
    // params: new_value, plus the non-null PK component only — the NULL
    // PK component does not consume a placeholder.
    assert_eq!(params, vec![Value::Text("y".into()), Value::Int(1)]);
}

#[test]
fn where_clause_all_null_pk_no_placeholders() {
    let columns = vec![col("a", true), col("b", true), col("c", false)];
    let original = vec![Value::Null, Value::Null, Value::Text("x".into())];
    let (sql, params) =
        build_single_cell_update("mysql", "t", &columns, &original, 2, Value::Text("y".into())).unwrap();
    assert_eq!(sql, "UPDATE `t` SET `c` = ? WHERE `a` IS NULL AND `b` IS NULL");
    assert_eq!(params, vec![Value::Text("y".into())]);
}

#[test]
fn insert_from_draft_returns_error_when_only_auto_columns() {
    let columns = vec![col_auto("id"), col_generated("calc")];
    let values = vec![Value::Null, Value::Null];
    let err = build_insert_from_draft("postgres", None, "t", &columns, &values).unwrap_err();
    assert!(matches!(err, BuildSqlError::NothingToUpdate));
}

#[test]
fn explain_uses_the_engine_plan_form() {
    assert_eq!(
        explain_statement("postgres", "SELECT 1").as_deref(),
        Some("EXPLAIN SELECT 1")
    );
    assert_eq!(
        explain_statement("sqlite", "  SELECT 1  ").as_deref(),
        Some("EXPLAIN QUERY PLAN SELECT 1")
    );
    assert_eq!(
        explain_statement("mysql", "SELECT 1").as_deref(),
        Some("EXPLAIN SELECT 1")
    );
}

#[test]
fn explain_is_unsupported_for_engines_without_a_single_statement_plan() {
    for driver_id in ["mssql", "mongodb", "redis"] {
        assert_eq!(explain_statement(driver_id, "SELECT 1"), None, "driver: {driver_id}");
    }
}

#[test]
fn explain_rejects_empty_sql() {
    assert_eq!(explain_statement("postgres", "   "), None);
}

#[test]
fn a_keyed_update_binds_new_values_then_every_key_component_in_order() {
    let columns = [col("tenant", true), col("note", false), col("code", true)];
    let (sql, params) = build_keyed_update(
        "mssql",
        Some("dbo"),
        "t",
        &columns,
        &[(1, Value::Text("edited".into()))],
        &[Value::Int(9_007_199_254_740_993), Value::Text("a".into())],
    )
    .unwrap();
    assert_eq!(
        sql,
        "UPDATE [dbo].[t] SET [note] = @P1 WHERE [tenant] = @P2 AND [code] = @P3"
    );
    assert_eq!(
        params,
        vec![
            Value::Text("edited".into()),
            Value::Int(9_007_199_254_740_993),
            Value::Text("a".into())
        ]
    );
}

#[test]
fn value_contract_postgres_text_array_updates_cast_through_text_to_a_builtin_array_type() {
    let mut columns = [col("id", true), col("value", false)];
    columns[1].data_type = "integer[]".into();
    let (sql, params) = build_keyed_update(
        "postgres",
        None,
        "t",
        &columns,
        &[(1, Value::Text("{3,NULL,5}".into()))],
        &[Value::Int(1)],
    )
    .unwrap();

    assert_eq!(
        sql,
        r#"UPDATE "t" SET "value" = $1::text::pg_catalog.int4[] WHERE "id" = $2"#
    );
    assert_eq!(params, vec![Value::Text("{3,NULL,5}".into()), Value::Int(1)]);

    let mut insert_columns = [col("value", false)];
    insert_columns[0].data_type = "float8[]".into();
    let (sql, params) = build_insert_from_draft(
        "postgres",
        None,
        "t",
        &insert_columns,
        &[Value::Text("{3,NULL,5}".into())],
    )
    .unwrap();
    assert_eq!(
        sql,
        r#"INSERT INTO "t" ("value") VALUES ($1::text::pg_catalog.float8[])"#
    );
    assert_eq!(params, vec![Value::Text("{3,NULL,5}".into())]);

    let (sql, _) =
        build_insert_from_draft("mysql", None, "t", &insert_columns, &[Value::Text("{3,NULL,5}".into())]).unwrap();
    assert_eq!(sql, "INSERT INTO `t` (`value`) VALUES (?)");
}

#[path = "sql_dialect/tests/postgres_array_cast_tests.rs"]
mod postgres_array_cast_tests;

#[path = "sql_dialect/tests/postgres_domain_cast_tests.rs"]
mod postgres_domain_cast_tests;

#[path = "sql_dialect/tests/postgres_temporal_cast_tests.rs"]
mod postgres_temporal_cast_tests;

#[test]
fn postgres_typed_numeric_updates_keep_the_native_placeholder() {
    let mut columns = [col("id", true), col("amount", false)];
    columns[1].data_type = "numeric(80, 40)".into();
    let value = Value::Decimal("12.50".parse().unwrap());
    let (sql, params) = build_keyed_update(
        "postgres",
        None,
        "ledger",
        &columns,
        &[(1, value.clone())],
        &[Value::Int(7)],
    )
    .unwrap();

    assert_eq!(sql, r#"UPDATE "ledger" SET "amount" = $1 WHERE "id" = $2"#);
    assert_eq!(params, vec![value, Value::Int(7)]);
}

#[test]
fn postgres_numeric_cast_does_not_interpolate_untrusted_metadata() {
    assert_eq!(
        postgres_numeric_cast_type("NUMERIC(80, 40)"),
        Some("pg_catalog.numeric")
    );
    assert_eq!(postgres_numeric_cast_type("decimal(10)"), Some("pg_catalog.numeric"));
    for type_name in ["text", "numeric(80, 40); DROP TABLE t", "numeric(,)"] {
        assert_eq!(postgres_numeric_cast_type(type_name), None, "{type_name}");
    }
    let at_limit = format!("numeric({}1,2)", " ".repeat(64 - "numeric(1,2)".len()));
    assert_eq!(at_limit.len(), 64);
    assert_eq!(postgres_numeric_cast_type(&at_limit), Some("pg_catalog.numeric"));
    let over_limit = format!("numeric({}1,2)", " ".repeat(53));
    assert_eq!(over_limit.len(), 65);
    assert_eq!(postgres_numeric_cast_type(&over_limit), None);

    let mut columns = [col("id", true), col("amount", false)];
    columns[1].data_type = "text".into();
    let (sql, _) = build_keyed_update(
        "postgres",
        None,
        "ledger",
        &columns,
        &[(1, Value::Text("x".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(sql, r#"UPDATE "ledger" SET "amount" = $1 WHERE "id" = $2"#);
}

#[test]
fn a_keyed_delete_matches_a_null_key_component_with_is_null() {
    let columns = [col("a", true), col("b", true)];
    let (sql, params) = build_keyed_delete("postgres", None, "t", &columns, &[Value::Null, Value::Int(2)]).unwrap();
    assert_eq!(sql, r#"DELETE FROM "t" WHERE "a" IS NULL AND "b" = $1"#);
    assert_eq!(params, vec![Value::Int(2)]);
}

#[test]
fn keyed_statements_refuse_a_key_or_column_that_no_longer_fits_the_table() {
    let columns = [col("id", true), col("note", false)];
    assert!(matches!(
        build_keyed_delete("postgres", None, "t", &[col("note", false)], &[Value::Int(1)]),
        Err(BuildSqlError::NoPrimaryKey)
    ));
    assert!(matches!(
        build_keyed_delete("postgres", None, "t", &columns, &[Value::Int(1), Value::Int(2)]),
        Err(BuildSqlError::StaleColumns)
    ));
    assert!(matches!(
        build_keyed_update("postgres", None, "t", &columns, &[(2, Value::Null)], &[Value::Int(1)]),
        Err(BuildSqlError::StaleColumns)
    ));
}
