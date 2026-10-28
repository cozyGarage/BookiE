use super::*;

pub(super) fn extract_named_parameters(sql: &str, driver_id: &str) -> NamedParameters {
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let sql = sql.to_string();
    let driver_id = driver_id.to_string();
    std::thread::spawn(move || {
        let result = super::extract_named_parameters(&sql, &driver_id);
        let _ = send.send(result);
    });
    receive
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("parameter extraction must terminate")
}

#[test]
fn value_contract_named_parameters_do_not_round_decimals() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/data/value-contract.json"
    )))
    .unwrap();
    for text in corpus["decimals"].as_array().unwrap() {
        let text = text.as_str().unwrap();
        let expected = Value::Decimal(text.parse().unwrap());
        assert_eq!(parse_parameter_value(ParameterKind::Decimal, text).unwrap(), expected);
        assert_eq!(parse_parameter_value(ParameterKind::Auto, text).unwrap(), expected);
    }
    for (input, expected) in [("1e3", "1000"), ("1.25e-2", "0.0125"), ("-1.25E2", "-125")] {
        let expected = Value::Decimal(expected.parse().unwrap());
        assert_eq!(parse_parameter_value(ParameterKind::Decimal, input).unwrap(), expected);
        assert_eq!(parse_parameter_value(ParameterKind::Auto, input).unwrap(), expected);
    }
    assert!(parse_parameter_value(ParameterKind::Decimal, "0.123456789012345678901234567891e1").is_err());
    for text in corpus["decimal_rejected"].as_array().unwrap() {
        let text = text.as_str().unwrap();
        assert!(parse_parameter_value(ParameterKind::Decimal, text).is_err());
        assert_eq!(
            parse_parameter_value(ParameterKind::Auto, text).unwrap(),
            Value::Text(text.into())
        );
    }
}

#[test]
fn value_contract_decimal_parameter_capacity_edges_remain_exact() {
    for text in ["79228162514264337593543950335", "-79228162514264337593543950335"] {
        assert!(matches!(
            parse_parameter_value(ParameterKind::Decimal, text),
            Ok(Value::Decimal(value)) if value.to_string() == text
        ));
        assert_eq!(
            parse_parameter_value(ParameterKind::Auto, text).unwrap(),
            Value::Text(text.into())
        );
    }
    let text = "0.0000000000000000000000000001";
    for kind in [ParameterKind::Decimal, ParameterKind::Auto] {
        assert!(matches!(
            parse_parameter_value(kind, text),
            Ok(Value::Decimal(value)) if value.to_string() == text
        ));
    }
    for text in [
        "79228162514264337593543950336",
        "-79228162514264337593543950336",
        "0.00000000000000000000000000001",
    ] {
        assert!(parse_parameter_value(ParameterKind::Decimal, text).is_err(), "{text}");
        assert_eq!(
            parse_parameter_value(ParameterKind::Auto, text).unwrap(),
            Value::Text(text.into())
        );
    }
}

#[test]
fn rewrites_named_parameters_per_dialect() {
    let sql = "SELECT * FROM users WHERE name = :name AND age > :age";
    let postgres = extract_named_parameters(sql, "postgres");
    assert_eq!(postgres.sql, "SELECT * FROM users WHERE name = $1 AND age > $2");
    assert_eq!(postgres.names, vec!["name", "age"]);
    assert_eq!(postgres.bindings, vec!["name", "age"]);

    let mysql = extract_named_parameters(sql, "mysql");
    assert_eq!(mysql.sql, "SELECT * FROM users WHERE name = ? AND age > ?");

    let mssql = extract_named_parameters(sql, "mssql");
    assert_eq!(mssql.sql, "SELECT * FROM users WHERE name = @P1 AND age > @P2");
}

#[test]
fn repeated_names_bind_once_per_occurrence() {
    let parsed = extract_named_parameters("SELECT :id, :id FROM t WHERE id = :id", "postgres");
    assert_eq!(parsed.sql, "SELECT $1, $2 FROM t WHERE id = $3");
    assert_eq!(parsed.names, vec!["id"]);
    assert_eq!(parsed.bindings, vec!["id", "id", "id"]);
}

#[test]
fn leaves_postgres_casts_and_assignments_alone() {
    let parsed = extract_named_parameters("SELECT '1'::int, x := 2 FROM t", "postgres");
    assert!(parsed.is_empty());
    assert_eq!(parsed.sql, "SELECT '1'::int, x := 2 FROM t");
}

#[test]
fn leaves_existing_positional_placeholders_alone() {
    let parsed = extract_named_parameters("SELECT * FROM t WHERE a = $1 AND b = ?", "postgres");
    assert!(parsed.is_empty());
    assert_eq!(parsed.sql, "SELECT * FROM t WHERE a = $1 AND b = ?");
}

#[test]
fn ignores_placeholders_inside_literals_and_comments() {
    let sql = "SELECT ':not_a_param', \"col:not\" -- :nope\n, /* :nope2 */ x FROM t WHERE y = :yes";
    let parsed = extract_named_parameters(sql, "postgres");
    assert_eq!(parsed.names, vec!["yes"]);
    assert!(parsed.sql.contains("':not_a_param'"));
    assert!(parsed.sql.contains("\"col:not\""));
    assert!(parsed.sql.contains("-- :nope"));
    assert!(parsed.sql.contains("/* :nope2 */"));
    assert!(parsed.sql.ends_with("y = $1"));
}

#[test]
fn ignores_placeholders_inside_dialect_quoting() {
    let mysql = extract_named_parameters("SELECT `col:not` FROM t WHERE a = :a", "mysql");
    assert_eq!(mysql.names, vec!["a"]);
    assert_eq!(mysql.sql, "SELECT `col:not` FROM t WHERE a = ?");

    let mssql = extract_named_parameters("SELECT [col:not] FROM t WHERE a = :a", "mssql");
    assert_eq!(mssql.names, vec!["a"]);
    assert_eq!(mssql.sql, "SELECT [col:not] FROM t WHERE a = @P1");
}

#[test]
fn ignores_placeholders_inside_dollar_quoted_bodies() {
    let sql = "CREATE FUNCTION f() RETURNS int AS $body$ SELECT :nope $body$ LANGUAGE sql";
    let parsed = extract_named_parameters(sql, "postgres");
    assert!(parsed.is_empty());
    assert_eq!(parsed.sql, sql);
}

#[test]
fn clickhouse_heredocs_hide_placeholders_through_their_close_or_end_of_input() {
    let closed = extract_named_parameters("SELECT $tag$ :hidden $tag$; SELECT :visible", "clickhouse");
    assert_eq!(closed.names, ["visible"]);
    assert_eq!(closed.sql, "SELECT $tag$ :hidden $tag$; SELECT ?");

    let unterminated = extract_named_parameters("SELECT :safe; SELECT $tag$unfinished :tail", "clickhouse");
    assert_eq!(unterminated.names, ["safe"]);
    assert_eq!(unterminated.sql, "SELECT ?; SELECT $tag$unfinished :tail");
}

#[test]
fn adjacent_positional_placeholders_are_not_dollar_quotes() {
    let parsed = extract_named_parameters("SELECT $1$2 FROM t WHERE a = :a", "postgres");
    assert_eq!(parsed.names, vec!["a"]);
    assert_eq!(parsed.sql, "SELECT $1$2 FROM t WHERE a = $1");
}

#[test]
fn ignores_doubled_quotes_inside_literals() {
    let parsed = extract_named_parameters("SELECT 'it''s :not' , :yes FROM t", "postgres");
    assert_eq!(parsed.names, vec!["yes"]);
    assert_eq!(parsed.sql, "SELECT 'it''s :not' , $1 FROM t");
}

#[test]
fn unterminated_literal_does_not_produce_parameters() {
    let parsed = extract_named_parameters("SELECT 'open :nope", "postgres");
    assert!(parsed.is_empty());
    assert_eq!(parsed.sql, "SELECT 'open :nope");
}

#[test]
fn a_mysql_hash_comment_hides_a_parameter() {
    let parsed = extract_named_parameters("SELECT 1 # :nope\n, :yes FROM t", "mysql");
    assert_eq!(parsed.names, vec!["yes"]);
    assert!(parsed.sql.contains("# :nope"));
}

#[test]
fn array_slices_are_not_parameters() {
    let parsed = extract_named_parameters("SELECT arr[1:3] FROM t", "postgres");
    assert!(parsed.is_empty());
}

#[test]
fn multibyte_text_keeps_its_bytes() {
    let parsed = extract_named_parameters("SELECT 'héllo →' , :naïve FROM t", "postgres");
    assert_eq!(parsed.names, vec!["na"]);
    assert!(parsed.sql.contains("'héllo →'"));
}

#[test]
fn explicit_kinds_parse_or_report_a_reason() {
    assert_eq!(
        parse_parameter_value(ParameterKind::Integer, " 42 "),
        Ok(Value::Int(42))
    );
    assert!(parse_parameter_value(ParameterKind::Integer, "x").is_err());
    assert_eq!(
        parse_parameter_value(ParameterKind::Decimal, "1.5"),
        Ok(Value::Decimal("1.5".parse().unwrap()))
    );
    assert!(parse_parameter_value(ParameterKind::Decimal, "nan").is_err());
    assert_eq!(
        parse_parameter_value(ParameterKind::Boolean, "TRUE"),
        Ok(Value::Bool(true))
    );
    assert!(parse_parameter_value(ParameterKind::Boolean, "maybe").is_err());
    assert_eq!(parse_parameter_value(ParameterKind::Null, "ignored"), Ok(Value::Null));
    assert_eq!(
        parse_parameter_value(ParameterKind::Text, "42"),
        Ok(Value::Text("42".into()))
    );
}

#[test]
fn auto_infers_numbers_and_keeps_everything_else_as_text() {
    assert_eq!(parse_parameter_value(ParameterKind::Auto, "7"), Ok(Value::Int(7)));
    assert_eq!(parse_parameter_value(ParameterKind::Auto, "-7"), Ok(Value::Int(-7)));
    assert_eq!(
        parse_parameter_value(ParameterKind::Auto, "2.5"),
        Ok(Value::Decimal("2.5".parse().unwrap()))
    );
    assert_eq!(
        parse_parameter_value(ParameterKind::Auto, "true"),
        Ok(Value::Text("true".into()))
    );
    assert_eq!(
        parse_parameter_value(ParameterKind::Auto, "' OR 1=1 --"),
        Ok(Value::Text("' OR 1=1 --".into()))
    );
}

#[test]
fn parameter_kind_index_round_trips() {
    for kind in ParameterKind::ALL {
        assert_eq!(ParameterKind::from_index(kind.index()), kind);
    }
    assert_eq!(ParameterKind::from_index(99), ParameterKind::Auto);
}
