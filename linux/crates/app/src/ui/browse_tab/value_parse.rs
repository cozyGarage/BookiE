use tablepro_core::{ColumnInfo, Value};

/// Collapse newlines / carriage returns to spaces, then squash any
/// resulting consecutive whitespace runs to a single space. Applied
/// at cell-edit commit time for non-JSON columns so a multi-line
/// clipboard paste into a single-line cell never reaches the SQL
/// layer with embedded `\n` — driver behaviour for that case is
/// type-specific (text columns store literally; numeric / date
/// columns parse-fail) and worth normalising up front.
pub(super) fn normalize_single_line_input(text: &str) -> String {
    let replaced: String = text
        .chars()
        .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
        .collect();
    replaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn parse_input_for_column(text: &str, col: Option<&ColumnInfo>) -> Result<Value, String> {
    let Some(col) = col else {
        return Ok(Value::Text(text.to_string()));
    };
    if text.is_empty() {
        if col.nullable || col.default_value.is_some() {
            return Ok(Value::Null);
        }
        return Err(crate::tr!("Field is required"));
    }
    let dt = col.data_type.to_ascii_lowercase();
    let trimmed = text.trim();
    if let Some(width) = mysql_bit_width(&dt) {
        return parse_mysql_bit_value(trimmed, width);
    }
    match classify_type(&dt) {
        TypeKind::Bool => parse_bool_value(trimmed),
        TypeKind::Int => parse_int_value(trimmed),
        TypeKind::Float => parse_float_value(trimmed),
        TypeKind::Decimal => parse_decimal_value(trimmed),
        TypeKind::Uuid => parse_uuid_value(trimmed),
        TypeKind::Json => parse_json_value(trimmed),
        TypeKind::TimestampTz => parse_timestamptz_value(trimmed),
        TypeKind::DateTime => parse_datetime_value(trimmed),
        TypeKind::Date => parse_date_value(trimmed),
        TypeKind::Time => parse_time_value(trimmed),
        TypeKind::Text => Ok(Value::Text(text.to_string())),
    }
}

pub(super) fn parse_input_for_driver(text: &str, col: Option<&ColumnInfo>, driver_id: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if driver_id == "postgres" && col.is_some_and(|column| is_postgres_numeric_type(&column.data_type)) {
        if matches!(trimmed, "NaN" | "Infinity" | "-Infinity") {
            return Ok(Value::Text(trimmed.into()));
        }
        return match parse_decimal_value(trimmed) {
            Ok(value) => Ok(value),
            Err(_) if is_postgres_numeric_literal(trimmed) => Ok(Value::Text(trimmed.into())),
            Err(error) => Err(error),
        };
    }
    parse_input_for_column(text, col)
}

fn is_postgres_numeric_type(data_type: &str) -> bool {
    let data_type = data_type.trim().to_ascii_lowercase();
    matches!(data_type.as_str(), "numeric" | "decimal")
        || data_type.starts_with("numeric(")
        || data_type.starts_with("decimal(")
}

fn is_postgres_numeric_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut cursor = usize::from(bytes.first().is_some_and(|byte| matches!(*byte, b'+' | b'-')));
    let integer_start = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let has_integer = cursor > integer_start;
    let mut has_fraction = false;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let fraction_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        has_fraction = cursor > fraction_start;
    }
    if !has_integer && !has_fraction {
        return false;
    }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) {
            cursor += 1;
        }
        let exponent_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == exponent_start {
            return false;
        }
    }
    cursor == bytes.len()
}

fn mysql_bit_width(data_type: &str) -> Option<u32> {
    data_type
        .strip_prefix("bit(")?
        .strip_suffix(')')?
        .parse::<u32>()
        .ok()
        .filter(|width| (1..=64).contains(width))
}

fn parse_mysql_bit_value(text: &str, width: u32) -> Result<Value, String> {
    if width == 1 {
        return parse_bool_value(text);
    }
    let number = text.parse::<u64>().map_err(|_| crate::tr!("Invalid integer"))?;
    let maximum = if width == 64 {
        i64::MAX as u64
    } else {
        (1u64 << width) - 1
    };
    if number > maximum {
        return Err(crate::tr!("Integer is outside the supported BIT range"));
    }
    Ok(Value::Int(number as i64))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TypeKind {
    Bool,
    Int,
    Float,
    Decimal,
    Uuid,
    Json,
    TimestampTz,
    DateTime,
    Date,
    Time,
    Text,
}

/// Map a lowercased `data_type` string to a coarse `TypeKind`. Order
/// of checks matters because several SQL types share substrings — for
/// example `timestamptz` / `timestamp with time zone` must be matched
/// before bare `timestamp`, and `tinyint(1)` (MySQL bool) must be
/// matched before generic `tinyint` / `int` patterns.
pub(super) fn classify_type(dt: &str) -> TypeKind {
    if let Some(kind) = classify_bool_or_bit(dt) {
        return kind;
    }
    if dt.contains("uuid") {
        return TypeKind::Uuid;
    }
    if is_json_like(dt) {
        return TypeKind::Json;
    }
    if let Some(kind) = classify_temporal(dt) {
        return kind;
    }
    classify_numeric(dt).unwrap_or(TypeKind::Text)
}

// Postgres `format_type()` returns "bit(1)" for length-1 BIT columns
// (not the bare "bit" the original guard expected). Both forms classify
// as Bool so the cell renders as a checkbox rather than a text editor
// that rejects "true"/"false" with "Invalid integer". Wider BIT(n)
// values are integers.
fn classify_bool_or_bit(dt: &str) -> Option<TypeKind> {
    if matches!(dt, "bool" | "boolean" | "bit" | "bit(1)" | "tinyint(1)") {
        return Some(TypeKind::Bool);
    }
    if dt.starts_with("bit(") {
        return Some(TypeKind::Int);
    }
    None
}

fn is_json_like(dt: &str) -> bool {
    dt.contains("json")
        || matches!(
            dt,
            "object"
                | "array"
                | "objectid"
                | "bsontimestamp"
                | "regex"
                | "javascript"
                | "javascriptwithscope"
                | "symbol"
                | "undefined"
                | "dbpointer"
                | "minkey"
                | "maxkey"
        )
}

fn classify_temporal(dt: &str) -> Option<TypeKind> {
    if dt.contains("timestamptz") || dt.contains("with time zone") {
        return Some(TypeKind::TimestampTz);
    }
    if dt.contains("timestamp") || dt.contains("datetime") {
        return Some(TypeKind::DateTime);
    }
    if dt == "date" || (dt.starts_with("date") && !dt.contains("datetime") && !dt.contains("time")) {
        return Some(TypeKind::Date);
    }
    if dt == "time" || dt.starts_with("time(") || dt == "time without time zone" {
        return Some(TypeKind::Time);
    }
    None
}

fn classify_numeric(dt: &str) -> Option<TypeKind> {
    if matches!(dt, "decimal" | "numeric" | "money") || dt.starts_with("decimal(") || dt.starts_with("numeric(") {
        return Some(TypeKind::Decimal);
    }
    if matches!(dt, "float" | "double" | "real" | "double precision") || dt.starts_with("float(") {
        return Some(TypeKind::Float);
    }
    if is_integer_type(dt) {
        return Some(TypeKind::Int);
    }
    None
}

fn is_integer_type(dt: &str) -> bool {
    matches!(
        dt,
        "int"
            | "int2"
            | "int4"
            | "int8"
            | "integer"
            | "smallint"
            | "bigint"
            | "tinyint"
            | "mediumint"
            | "serial"
            | "bigserial"
            | "smallserial"
    ) || dt.starts_with("int(")
        || dt.starts_with("integer(")
        || dt.starts_with("smallint(")
        || dt.starts_with("bigint(")
        || dt.starts_with("tinyint(")
        || dt.starts_with("mediumint(")
}

pub(super) fn parse_bool_value(text: &str) -> Result<Value, String> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "t" | "1" | "yes" | "y" | "on" => Ok(Value::Bool(true)),
        "false" | "f" | "0" | "no" | "n" | "off" => Ok(Value::Bool(false)),
        _ => Err(crate::tr!("Invalid boolean. Use true/false, yes/no, or 1/0.")),
    }
}

pub(super) fn parse_int_value(text: &str) -> Result<Value, String> {
    text.parse::<i64>()
        .map(Value::Int)
        .map_err(|_| crate::tr!("Invalid integer"))
}

pub(super) fn parse_float_value(text: &str) -> Result<Value, String> {
    tablepro_core::parse_float_input(text)
        .map(Value::Float)
        .map_err(|_| crate::tr!("Invalid number"))
}

pub(super) fn parse_decimal_value(text: &str) -> Result<Value, String> {
    rust_decimal::Decimal::from_str_exact(text)
        .map(Value::Decimal)
        .map_err(|_| crate::tr!("Invalid decimal"))
}

pub(super) fn parse_uuid_value(text: &str) -> Result<Value, String> {
    uuid::Uuid::parse_str(text)
        .map(Value::Uuid)
        .map_err(|_| crate::tr!("Invalid UUID. Expected 8-4-4-4-12 hex digits."))
}

pub(super) fn parse_json_value(text: &str) -> Result<Value, String> {
    serde_json::from_str::<serde_json::Value>(text)
        .map(Value::Json)
        .map_err(|e| crate::tr!("Invalid JSON: {error}").replace("{error}", &e.to_string()))
}

pub(super) fn parse_timestamptz_value(text: &str) -> Result<Value, String> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(text) {
        return Ok(Value::TimestampTz(dt.with_timezone(&chrono::Utc)));
    }
    Err(crate::tr!(
        "Invalid timestamp. Use ISO 8601, e.g. 2024-01-15T14:30:00Z."
    ))
}

pub(super) fn parse_datetime_value(text: &str) -> Result<Value, String> {
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ];
    for fmt in &formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(text, fmt) {
            return Ok(Value::DateTime(dt));
        }
    }
    Err(crate::tr!("Invalid datetime. Use YYYY-MM-DD HH:MM:SS."))
}

pub(super) fn parse_date_value(text: &str) -> Result<Value, String> {
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map(Value::Date)
        .map_err(|_| crate::tr!("Invalid date. Use YYYY-MM-DD."))
}

pub(super) fn parse_time_value(text: &str) -> Result<Value, String> {
    let formats = ["%H:%M:%S", "%H:%M:%S%.f", "%H:%M"];
    for fmt in &formats {
        if let Ok(t) = chrono::NaiveTime::parse_from_str(text, fmt) {
            return Ok(Value::Time(t));
        }
    }
    Err(crate::tr!("Invalid time. Use HH:MM:SS."))
}

#[cfg(test)]
#[path = "../../../../core/tests/support/parser_contract.rs"]
mod parser_contract;

#[cfg(test)]
mod tests {
    use super::{
        TypeKind, classify_type, normalize_single_line_input, parse_decimal_value, parse_input_for_column,
        parse_input_for_driver,
    };
    use tablepro_core::{ColumnInfo, Value};

    #[tokio::test]
    async fn sqlite_numeric_grid_edit_keeps_parser_and_affinity_behavior() {
        use tablepro_core::{ConnectOptions, DatabaseDriver};

        let connection = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: ":memory:".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        connection
            .execute("CREATE TABLE flexible (id INTEGER PRIMARY KEY, amount NUMERIC)")
            .await
            .unwrap();
        connection.execute("INSERT INTO flexible VALUES (1, 0)").await.unwrap();

        let columns = connection.fetch_columns(None, "flexible").await.unwrap();
        let amount_index = columns.iter().position(|column| column.name == "amount").unwrap();
        let edit = parse_input_for_column("42.50", Some(&columns[amount_index])).unwrap();
        assert_eq!(edit, Value::Decimal("42.50".parse().unwrap()));
        let update = tablepro_core::sql_dialect::build_keyed_update(
            "sqlite",
            None,
            "flexible",
            &columns,
            &[(amount_index, edit)],
            &[Value::Int(1)],
        )
        .unwrap();
        connection.execute_in_transaction(&[update]).await.unwrap();

        let saved = connection
            .query("SELECT typeof(amount), amount FROM flexible WHERE id = 1")
            .await
            .unwrap();
        assert_eq!(saved.rows, vec![vec![Value::Text("real".into()), Value::Float(42.5)]]);
    }

    #[test]
    fn value_contract_parser_preserves_boundaries_and_rejects_rounding() {
        super::parser_contract::assert_numeric_parsers(
            |text| super::parse_int_value(text).ok(),
            |text| super::parse_decimal_value(text).ok(),
            |text| super::parse_float_value(text).ok(),
        );
    }

    #[test]
    fn decimal_preservation_rejects_an_edit_that_would_round() {
        assert!(super::parse_decimal_value("0.123456789012345678901234567891").is_err());
        assert!(super::parse_decimal_value("12.3400").is_ok());
    }

    #[test]
    fn postgres_numeric_specials_remain_exact_text_only_for_postgres() {
        let column = col("numeric", false);
        for input in ["NaN", "Infinity", "-Infinity"] {
            assert_eq!(
                parse_input_for_driver(input, Some(&column), "postgres"),
                Ok(Value::Text(input.into()))
            );
            assert!(parse_input_for_driver(input, Some(&column), "mysql").is_err());
        }
        assert_eq!(
            parse_input_for_driver("12.50", Some(&column), "postgres"),
            Ok(Value::Decimal("12.50".parse().unwrap()))
        );
    }

    #[test]
    fn postgres_wide_numeric_edits_stay_exact_text_when_decimal_cannot_represent_them() {
        let column = col("numeric(80,40)", false);
        let wide = "1234567890123456789012345678901234567890.1234567890123456789012345678901234567890";
        assert!(parse_decimal_value(wide).is_err());
        let parsed = parse_input_for_driver(wide, Some(&column), "postgres").unwrap();
        assert_eq!(parsed, Value::Text(wide.into()));
        let mut columns = vec![col("id", false), column.clone()];
        columns[0].primary_key = true;
        let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
            "postgres",
            None,
            "wide_numeric",
            &columns,
            &[(1, parsed)],
            &[Value::Int(1)],
        )
        .unwrap();
        assert!(sql.contains("$1::text::pg_catalog.numeric"));
        assert_eq!(params[0], Value::Text(wide.into()));
        let max_precision = format!("0.{}", "1234567890".repeat(100));
        assert!(parse_decimal_value(&max_precision).is_err());
        assert_eq!(
            parse_input_for_driver(&max_precision, Some(&col("numeric(1000,1000)", false)), "postgres"),
            Ok(Value::Text(max_precision.clone()))
        );
        assert!(parse_input_for_driver(wide, Some(&column), "mysql").is_err());
        for malformed in ["", ".", "+", "--1", "1e", "1e+", "1.2.3", "1x", "1; DROP TABLE t"] {
            assert!(
                parse_input_for_driver(malformed, Some(&column), "postgres").is_err(),
                "accepted malformed PostgreSQL numeric literal {malformed:?}"
            );
        }
        assert!(parse_input_for_driver(wide, Some(&col("money", false)), "postgres").is_err());
    }

    #[tokio::test]
    #[ignore = "requires docker"]
    async fn postgres_numeric_parser_outputs_round_trip_through_server() {
        use tablepro_core::{ConnectOptions, Connection, DatabaseDriver};
        use testcontainers::ImageExt;
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::postgres::Postgres;

        const VALUE: &str = "1234567890123456789012345678901234567890.1234567890123456789012345678901234567890";
        let container = Postgres::default().with_tag("16-alpine").start().await.unwrap();
        let options = ConnectOptions {
            host: container.get_host().await.unwrap().to_string(),
            port: container.get_host_port_ipv4(5432).await.unwrap(),
            database: "postgres".into(),
            username: "postgres".into(),
            password: secrecy::SecretString::new("postgres".to_string().into()),
            ..Default::default()
        };
        let connection = drivers_postgres::PgDriver.connect(options).await.unwrap();
        for (table, data_type) in [
            ("parser_numeric", "numeric(80, 40)"),
            ("parser_unconstrained_numeric", "numeric"),
        ] {
            connection
                .execute(&format!(
                    "CREATE TABLE {table} (id integer PRIMARY KEY, amount {data_type})"
                ))
                .await
                .unwrap();
            connection
                .execute(&format!("INSERT INTO {table} VALUES (1, 0)"))
                .await
                .unwrap();
            let columns = connection.fetch_columns(None, table).await.unwrap();
            let amount = columns.iter().position(|column| column.name == "amount").unwrap();
            let id = columns.iter().position(|column| column.name == "id").unwrap();
            assert!(columns[id].primary_key);
            if data_type == "numeric" {
                assert_eq!(columns[amount].data_type, "numeric");
            }
            let parsed = parse_input_for_driver(VALUE, Some(&columns[amount]), "postgres").unwrap();
            assert_eq!(parsed, Value::Text(VALUE.into()));
            let update = tablepro_core::sql_dialect::build_keyed_update(
                "postgres",
                None,
                table,
                &columns,
                &[(amount, parsed)],
                &[Value::Int(1)],
            )
            .unwrap();
            assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

            let saved = connection
                .query(&format!("SELECT amount::text FROM {table} WHERE id = 1"))
                .await
                .unwrap();
            assert_eq!(saved.rows, vec![vec![Value::Text(VALUE.into())]], "{data_type}");
        }
    }

    #[test]
    fn postgres_text_array_grid_literal_stays_text_for_the_shared_cast() {
        let literal = r#"{"plain",NULL,"quote \" slash \\, comma"}"#;
        let mut columns = vec![col("id", false), col("text[]", false)];
        columns[0].primary_key = true;
        let parsed = parse_input_for_driver(literal, Some(&columns[1]), "postgres").unwrap();
        assert_eq!(parsed, Value::Text(literal.into()));
        let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
            "postgres",
            None,
            "text_array_grid",
            &columns,
            &[(1, parsed)],
            &[Value::Int(1)],
        )
        .unwrap();
        assert!(sql.contains("$1::text::pg_catalog.text[]"));
        assert_eq!(params[0], Value::Text(literal.into()));
    }

    #[test]
    fn postgres_numeric_array_grid_literal_stays_text_for_the_shared_cast() {
        let literal =
            r#"{1234567890123456789012345678901234567890.12345678901234567890,1.2300,NaN,Infinity,-Infinity,NULL}"#;
        let mut columns = vec![col("id", false), col("numeric[]", false)];
        columns[0].primary_key = true;
        let parsed = parse_input_for_driver(literal, Some(&columns[1]), "postgres").unwrap();
        assert_eq!(parsed, Value::Text(literal.into()));
        let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
            "postgres",
            None,
            "numeric_array_grid",
            &columns,
            &[(1, parsed)],
            &[Value::Int(1)],
        )
        .unwrap();
        assert!(sql.contains("$1::text::pg_catalog.numeric[]"));
        assert_eq!(params[0], Value::Text(literal.into()));
    }

    #[test]
    fn normalize_single_line_leaves_plain_text_untouched() {
        assert_eq!(normalize_single_line_input("hello world"), "hello world");
    }

    #[test]
    fn normalize_single_line_collapses_a_multiline_paste_into_one_line() {
        assert_eq!(
            normalize_single_line_input("first\nsecond\r\nthird"),
            "first second third"
        );
    }

    #[test]
    fn normalize_single_line_squashes_runs_of_whitespace_left_by_the_collapse() {
        assert_eq!(normalize_single_line_input("a\n\n\nb"), "a b");
        assert_eq!(
            normalize_single_line_input("  leading and trailing  "),
            "leading and trailing"
        );
    }

    fn col(data_type: &str, nullable: bool) -> ColumnInfo {
        ColumnInfo {
            name: "x".into(),
            data_type: data_type.into(),
            nullable,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        }
    }

    fn col_with_default(data_type: &str, default: &str) -> ColumnInfo {
        let mut c = col(data_type, false);
        c.default_value = Some(default.into());
        c
    }

    #[test]
    fn classify_disambiguates_overlapping_types() {
        assert_eq!(classify_type("tinyint(1)"), TypeKind::Bool);
        assert_eq!(classify_type("bit(1)"), TypeKind::Bool);
        assert_eq!(classify_type("bit(8)"), TypeKind::Int);
        assert_eq!(classify_type("bit(64)"), TypeKind::Int);
        assert_eq!(classify_type("tinyint"), TypeKind::Int);
        assert_eq!(classify_type("uuid"), TypeKind::Uuid);
        assert_eq!(classify_type("jsonb"), TypeKind::Json);
        assert_eq!(classify_type("object"), TypeKind::Json);
        assert_eq!(classify_type("array"), TypeKind::Json);
        assert_eq!(classify_type("objectid"), TypeKind::Json);
        for mongo_special in [
            "bsontimestamp",
            "regex",
            "javascript",
            "javascriptwithscope",
            "symbol",
            "undefined",
            "dbpointer",
            "minkey",
            "maxkey",
        ] {
            assert_eq!(classify_type(mongo_special), TypeKind::Json, "{mongo_special}");
        }
        assert_eq!(classify_type("timestamptz"), TypeKind::TimestampTz);
        assert_eq!(classify_type("timestamp with time zone"), TypeKind::TimestampTz);
        assert_eq!(classify_type("timestamp without time zone"), TypeKind::DateTime);
        assert_eq!(classify_type("timestamp"), TypeKind::DateTime);
        assert_eq!(classify_type("datetime"), TypeKind::DateTime);
        assert_eq!(classify_type("date"), TypeKind::Date);
        assert_eq!(classify_type("time"), TypeKind::Time);
        assert_eq!(classify_type("integer"), TypeKind::Int);
        assert_eq!(classify_type("int4"), TypeKind::Int);
        assert_eq!(classify_type("bigint"), TypeKind::Int);
        assert_eq!(classify_type("decimal(10,2)"), TypeKind::Decimal);
        assert_eq!(classify_type("numeric"), TypeKind::Decimal);
        assert_eq!(classify_type("double precision"), TypeKind::Float);
        assert_eq!(classify_type("real"), TypeKind::Float);
        assert_eq!(classify_type("text"), TypeKind::Text);
        assert_eq!(classify_type("varchar(255)"), TypeKind::Text);
        // "interval" must NOT be classified as Int even though it
        // contains "int".
        assert_eq!(classify_type("interval"), TypeKind::Text);
    }

    #[test]
    fn empty_on_nullable_yields_null() {
        let r = parse_input_for_column("", Some(&col("text", true))).unwrap();
        assert!(matches!(r, Value::Null));
    }

    #[test]
    fn empty_on_not_null_with_default_yields_null() {
        let r = parse_input_for_column("", Some(&col_with_default("timestamp", "now()"))).unwrap();
        assert!(matches!(r, Value::Null));
    }

    #[test]
    fn empty_on_not_null_no_default_is_rejected() {
        let r = parse_input_for_column("", Some(&col("text", false)));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("required"));
    }

    #[test]
    fn parses_int_decimal_float_bool() {
        assert!(matches!(
            parse_input_for_column("42", Some(&col("integer", false))).unwrap(),
            Value::Int(42)
        ));
        assert!(matches!(
            parse_input_for_column("3.14", Some(&col("real", false))).unwrap(),
            Value::Float(_)
        ));
        assert!(matches!(
            parse_input_for_column("99.99", Some(&col("decimal(10,2)", false))).unwrap(),
            Value::Decimal(_)
        ));
        assert!(matches!(
            parse_input_for_column("yes", Some(&col("boolean", false))).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            parse_input_for_column("0", Some(&col("tinyint(1)", false))).unwrap(),
            Value::Bool(false)
        ));
        assert_eq!(
            parse_input_for_column("1", Some(&col("bit(1)", false))).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            parse_input_for_column("0", Some(&col("bit(1)", false))).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            parse_input_for_column("170", Some(&col("bit(8)", false))).unwrap(),
            Value::Int(170)
        );
        assert!(parse_input_for_column("256", Some(&col("bit(8)", false))).is_err());
        assert!(parse_input_for_column("-1", Some(&col("bit(8)", false))).is_err());
        assert_eq!(
            parse_input_for_column("9223372036854775807", Some(&col("bit(64)", false))).unwrap(),
            Value::Int(i64::MAX)
        );
        assert!(parse_input_for_column("9223372036854775808", Some(&col("bit(64)", false))).is_err());
    }

    #[test]
    fn parses_uuid_json_date_time_datetime_timestamptz() {
        let uuid = parse_input_for_column("550e8400-e29b-41d4-a716-446655440000", Some(&col("uuid", false))).unwrap();
        assert!(matches!(uuid, Value::Uuid(_)));

        let json = parse_input_for_column(r#"{"a":1}"#, Some(&col("jsonb", false))).unwrap();
        assert!(matches!(json, Value::Json(_)));

        let mongo_object = parse_input_for_column(
            r#"{"amount":{"$numberDecimal":"12.3400"}}"#,
            Some(&col("object", false)),
        )
        .unwrap();
        assert_eq!(
            mongo_object,
            Value::Json(serde_json::json!({"amount": {"$numberDecimal": "12.3400"}}))
        );
        assert_eq!(
            parse_input_for_column(r#"{"$oid":"507f1f77bcf86cd799439011"}"#, Some(&col("ObjectId", false))).unwrap(),
            Value::Json(serde_json::json!({"$oid": "507f1f77bcf86cd799439011"}))
        );
        assert_eq!(
            parse_input_for_column(r#"[{"ordinal":{"$numberLong":"7"}}]"#, Some(&col("array", false))).unwrap(),
            Value::Json(serde_json::json!([{"ordinal": {"$numberLong": "7"}}]))
        );

        let date = parse_input_for_column("2024-01-15", Some(&col("date", false))).unwrap();
        assert!(matches!(date, Value::Date(_)));

        let time = parse_input_for_column("14:30:00", Some(&col("time", false))).unwrap();
        assert!(matches!(time, Value::Time(_)));
        let time_short = parse_input_for_column("14:30", Some(&col("time", false))).unwrap();
        assert!(matches!(time_short, Value::Time(_)));

        let datetime = parse_input_for_column("2024-01-15 14:30:00", Some(&col("timestamp", false))).unwrap();
        assert!(matches!(datetime, Value::DateTime(_)));
        let datetime_t = parse_input_for_column("2024-01-15T14:30:00", Some(&col("datetime", false))).unwrap();
        assert!(matches!(datetime_t, Value::DateTime(_)));

        let ts = parse_input_for_column("2024-01-15T14:30:00Z", Some(&col("timestamptz", false))).unwrap();
        assert!(matches!(ts, Value::TimestampTz(_)));
    }

    #[test]
    fn rejects_invalid_type_specific_input() {
        assert!(parse_input_for_column("not-a-number", Some(&col("integer", false))).is_err());
        assert!(parse_input_for_column("not-a-uuid", Some(&col("uuid", false))).is_err());
        assert!(parse_input_for_column("{not json", Some(&col("jsonb", false))).is_err());
        assert!(parse_input_for_column("2024/01/15", Some(&col("date", false))).is_err());
        assert!(parse_input_for_column("13:00:99", Some(&col("time", false))).is_err());
        assert!(parse_input_for_column("not-a-date", Some(&col("timestamp", false))).is_err());
        assert!(parse_input_for_column("maybe", Some(&col("boolean", false))).is_err());
    }

    #[test]
    fn unknown_type_falls_through_to_text() {
        let r = parse_input_for_column("anything goes here", Some(&col("varchar(255)", false))).unwrap();
        assert!(matches!(r, Value::Text(_)));
    }

    #[test]
    fn null_sentinel_typed_literally_is_text() {
        let r = parse_input_for_column("<NULL>", Some(&col("text", true))).unwrap();
        match r {
            Value::Text(s) => assert_eq!(s, "<NULL>"),
            other => panic!("expected Text(\"<NULL>\") got {other:?}"),
        }
    }
}
