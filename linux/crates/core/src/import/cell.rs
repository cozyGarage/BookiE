use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use thiserror::Error;
use uuid::Uuid;

use crate::import::csv_import::CsvImportOptions;
use crate::query::{ColumnInfo, Value};

/// What a field could not be turned into. Names the failure only: the cell
/// text never travels with it, because this reaches error lists and logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CellError {
    #[error("not true or false")]
    NotABoolean,
    #[error("not a whole number")]
    NotAnInteger,
    #[error("not a number")]
    NotANumber,
    #[error("not a date")]
    NotADate,
    #[error("not a time")]
    NotATime,
    #[error("not a timestamp")]
    NotATimestamp,
    #[error("not a UUID")]
    NotAUuid,
    #[error("not JSON")]
    NotJson,
    #[error("not hexadecimal bytes")]
    NotBytes,
}

/// A field that could not become a value, named well enough for the user
/// to find the cell without the message carrying its contents.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("line {line}, column {column}: {reason}")]
pub struct CsvRowError {
    /// The row's position in the file, counting the header as line 1 so it
    /// matches what a spreadsheet shows.
    pub line: usize,
    pub column: String,
    pub reason: CellError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Bool,
    Int,
    Float,
    Decimal,
    Date,
    Time,
    DateTime,
    TimestampTz,
    Uuid,
    Json,
    Bytes,
    Text,
}

impl ColumnKind {
    pub const ALL: [Self; 12] = [
        Self::Text,
        Self::Int,
        Self::Float,
        Self::Decimal,
        Self::Bool,
        Self::Date,
        Self::Time,
        Self::DateTime,
        Self::TimestampTz,
        Self::Uuid,
        Self::Json,
        Self::Bytes,
    ];
}

/// Read a catalog type name as the value shape the import must produce.
/// Anything unrecognised is text, which is the reading that never loses
/// what the file said.
pub fn column_kind(data_type: &str) -> ColumnKind {
    let lowered = data_type.trim().to_ascii_lowercase();
    // Array values are transported as their driver's textual array form.
    // Parsing the element type here would incorrectly treat the whole cell
    // (for example, "{1,2}") as a scalar number.
    if lowered.contains('[') || lowered.ends_with(" array") {
        return ColumnKind::Text;
    }
    let base = lowered.split(['(', '[']).next().unwrap_or("").trim().to_owned();
    if let Some(kind) = temporal_kind(&base, &lowered) {
        return kind;
    }
    if base.contains("bool") || base == "bit" {
        return ColumnKind::Bool;
    }
    if base.contains("uuid") || base.contains("uniqueidentifier") {
        return ColumnKind::Uuid;
    }
    if matches!(base.as_str(), "int128" | "uint128") {
        return ColumnKind::Text;
    }
    if base.contains("json") {
        return ColumnKind::Json;
    }
    if base.contains("bytea") || base.contains("blob") || base.contains("binary") || base == "image" {
        return ColumnKind::Bytes;
    }
    if base.contains("decimal") || base.contains("numeric") || base.contains("money") {
        return ColumnKind::Decimal;
    }
    if base.contains("int") || base.contains("serial") {
        return ColumnKind::Int;
    }
    if base.contains("float") || base.contains("double") || base.contains("real") {
        return ColumnKind::Float;
    }
    ColumnKind::Text
}

fn temporal_kind(base: &str, lowered: &str) -> Option<ColumnKind> {
    if base.contains("timestamptz") || lowered.contains("with time zone") || base.contains("datetimeoffset") {
        return Some(ColumnKind::TimestampTz);
    }
    if base.contains("timestamp") || base.contains("datetime") {
        return Some(ColumnKind::DateTime);
    }
    if base == "date" {
        return Some(ColumnKind::Date);
    }
    if base.contains("time") {
        return Some(ColumnKind::Time);
    }
    None
}

pub fn parse_cell(text: &str, kind: ColumnKind) -> Result<Value, CellError> {
    let trimmed = text.trim();
    match kind {
        ColumnKind::Text => Ok(Value::Text(text.to_owned())),
        ColumnKind::Bool => parse_bool(trimmed),
        ColumnKind::Int => trimmed.parse().map(Value::Int).map_err(|_| CellError::NotAnInteger),
        ColumnKind::Float => crate::parse_float_input(trimmed)
            .map(Value::Float)
            .map_err(|_| CellError::NotANumber),
        ColumnKind::Decimal => Decimal::from_str_exact(trimmed)
            .map(Value::Decimal)
            .map_err(|_| CellError::NotANumber),
        ColumnKind::Uuid => Uuid::parse_str(trimmed)
            .map(Value::Uuid)
            .map_err(|_| CellError::NotAUuid),
        ColumnKind::Json => serde_json::from_str(trimmed)
            .map(Value::Json)
            .map_err(|_| CellError::NotJson),
        ColumnKind::Bytes => parse_bytes(trimmed),
        ColumnKind::Date => NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
            .map(Value::Date)
            .map_err(|_| CellError::NotADate),
        ColumnKind::Time => parse_time(trimmed),
        ColumnKind::DateTime => parse_datetime(trimmed).map(Value::DateTime),
        ColumnKind::TimestampTz => parse_timestamptz(trimmed),
    }
}

fn parse_bool(text: &str) -> Result<Value, CellError> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "1" => Ok(Value::Bool(true)),
        "false" | "f" | "no" | "n" | "0" => Ok(Value::Bool(false)),
        _ => Err(CellError::NotABoolean),
    }
}

const TIME_FORMATS: [&str; 2] = ["%H:%M:%S%.f", "%H:%M"];

fn parse_time(text: &str) -> Result<Value, CellError> {
    TIME_FORMATS
        .iter()
        .find_map(|format| NaiveTime::parse_from_str(text, format).ok())
        .map(Value::Time)
        .ok_or(CellError::NotATime)
}

const DATETIME_FORMATS: [&str; 4] = [
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M",
    "%Y-%m-%dT%H:%M",
];

fn parse_datetime(text: &str) -> Result<NaiveDateTime, CellError> {
    if let Some(parsed) = DATETIME_FORMATS
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
    {
        return Ok(parsed);
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .ok_or(CellError::NotATimestamp)
}

fn parse_timestamptz(text: &str) -> Result<Value, CellError> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Ok(Value::TimestampTz(parsed.with_timezone(&Utc)));
    }
    parse_datetime(text).map(|naive| Value::TimestampTz(naive.and_utc()))
}

/// Binary columns take hexadecimal, with or without the `0x` prefix every
/// engine's own hex literal uses. There is no other text form a delimited
/// file can carry that round-trips through every driver.
fn parse_bytes(text: &str) -> Result<Value, CellError> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .or_else(|| text.strip_prefix("\\x"))
        .unwrap_or(text);
    if !digits.len().is_multiple_of(2) {
        return Err(CellError::NotBytes);
    }
    let mut out = Vec::with_capacity(digits.len() / 2);
    for pair in digits.as_bytes().chunks(2) {
        let pair = std::str::from_utf8(pair).map_err(|_| CellError::NotBytes)?;
        out.push(u8::from_str_radix(pair, 16).map_err(|_| CellError::NotBytes)?);
    }
    Ok(Value::Bytes(out))
}

/// Turn one record into the values a bound INSERT takes, one per column in
/// `columns`. A column with no mapped field, or one whose field is past the
/// end of a short row, becomes NULL, which the statement builder drops when
/// the column has a server default.
pub fn row_to_values(
    row: &[String],
    mapping: &[Option<usize>],
    columns: &[ColumnInfo],
    options: &CsvImportOptions,
    line: usize,
) -> Result<Vec<Value>, CsvRowError> {
    row_to_values_for_driver(row, mapping, columns, options, line, "")
}

pub(crate) fn row_to_values_for_driver(
    row: &[String],
    mapping: &[Option<usize>],
    columns: &[ColumnInfo],
    options: &CsvImportOptions,
    line: usize,
    driver_id: &str,
) -> Result<Vec<Value>, CsvRowError> {
    columns
        .iter()
        .zip(mapping)
        .map(|(column, field)| {
            let Some(text) = field.and_then(|index| row.get(index)) else {
                return Ok(Value::Null);
            };
            value_for(text, column, options, driver_id).map_err(|reason| CsvRowError {
                line,
                column: column.name.clone(),
                reason,
            })
        })
        .collect()
}

fn value_for(text: &str, column: &ColumnInfo, options: &CsvImportOptions, driver_id: &str) -> Result<Value, CellError> {
    let kind = column_kind(&column.data_type);
    if text != options.null_marker {
        if kind == ColumnKind::Text
            && let Some(value) = sanitized_wide_integer_text(text, &column.data_type)
        {
            return Ok(Value::Text(value.to_owned()));
        }
        if kind == ColumnKind::Decimal
            && let Some(value) = sanitized_decimal(text)
        {
            return Ok(Value::Decimal(value));
        }
        return match parse_cell(text, kind) {
            Ok(value) => Ok(value),
            Err(_)
                if driver_id == "sqlite"
                    && matches!(kind, ColumnKind::Int | ColumnKind::Float | ColumnKind::Decimal) =>
            {
                // SQLite affinity is advisory: a NUMERIC/INTEGER/REAL column
                // may legally store text when the input is not numeric.
                Ok(Value::Text(text.to_owned()))
            }
            Err(error) => Err(error),
        };
    }
    // Every export in this app writes NULL as an empty field, but so is an
    // empty string, and for text the empty string is the reading that loses
    // nothing.
    if options.null_marker.is_empty() && kind == ColumnKind::Text {
        return Ok(Value::Text(String::new()));
    }
    Ok(Value::Null)
}

fn sanitized_wide_integer_text<'a>(text: &'a str, data_type: &str) -> Option<&'a str> {
    let value = text.strip_prefix('\'')?;
    if !value.starts_with(['+', '-']) {
        return None;
    }
    if data_type.trim().eq_ignore_ascii_case("Int128") && value.parse::<i128>().is_ok() {
        return Some(value);
    }
    if data_type.trim().eq_ignore_ascii_case("UInt128") && value.parse::<u128>().is_ok() {
        return Some(value);
    }
    None
}

fn sanitized_decimal(text: &str) -> Option<Decimal> {
    Decimal::from_str_exact(text.strip_prefix('\'')?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.to_owned(),
            data_type: data_type.to_owned(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        }
    }

    #[test]
    fn value_contract_parser_preserves_boundaries_and_rejects_rounding() {
        let parse = |kind: ColumnKind| move |text: &str| parse_cell(text, kind).ok();
        crate::parser_contract::assert_numeric_parsers(
            parse(ColumnKind::Int),
            parse(ColumnKind::Decimal),
            parse(ColumnKind::Float),
        );
        for text in crate::parser_contract::corpus_texts("texts") {
            assert_eq!(parse_cell(&text, ColumnKind::Text).unwrap(), Value::Text(text.clone()));
        }
    }

    #[test]
    fn sqlite_numeric_affinity_text_fallback_does_not_apply_to_other_drivers() {
        let options = CsvImportOptions::default();
        for (data_type, expected_error) in [
            ("INTEGER", CellError::NotAnInteger),
            ("REAL", CellError::NotANumber),
            ("NUMERIC", CellError::NotANumber),
        ] {
            let target = column("value", data_type);
            assert_eq!(
                value_for("not numeric", &target, &options, "sqlite"),
                Ok(Value::Text("not numeric".into())),
                "SQLite affinity stores nonnumeric input as text for {data_type}"
            );
            assert_eq!(
                value_for("not numeric", &target, &options, "mysql").unwrap_err(),
                expected_error,
                "the SQLite fallback must not mask a type error for {data_type}"
            );
            assert_eq!(
                value_for("not numeric", &target, &options, "mssql").unwrap_err(),
                expected_error,
                "the SQLite fallback must not mask a type error for {data_type}"
            );
        }
    }

    #[test]
    fn a_catalog_type_name_reads_as_the_value_shape_it_stores() {
        assert_eq!(column_kind("BIGINT"), ColumnKind::Int);
        assert_eq!(column_kind("integer"), ColumnKind::Int);
        assert_eq!(column_kind("Int128"), ColumnKind::Text);
        assert_eq!(column_kind("UInt128"), ColumnKind::Text);
        assert_eq!(column_kind("bigserial"), ColumnKind::Int);
        assert_eq!(column_kind("VARCHAR(255)"), ColumnKind::Text);
        assert_eq!(column_kind("numeric(10,2)"), ColumnKind::Decimal);
        assert_eq!(column_kind("integer[]"), ColumnKind::Text);
        assert_eq!(column_kind("float8[]"), ColumnKind::Text);
        assert_eq!(column_kind("numeric ARRAY"), ColumnKind::Text);
        assert_eq!(column_kind("double precision"), ColumnKind::Float);
        assert_eq!(column_kind("boolean"), ColumnKind::Bool);
        assert_eq!(column_kind("date"), ColumnKind::Date);
        assert_eq!(column_kind("time without time zone"), ColumnKind::Time);
        assert_eq!(column_kind("timestamp"), ColumnKind::DateTime);
        assert_eq!(column_kind("timestamp with time zone"), ColumnKind::TimestampTz);
        assert_eq!(column_kind("timestamptz"), ColumnKind::TimestampTz);
        assert_eq!(column_kind("uuid"), ColumnKind::Uuid);
        assert_eq!(column_kind("jsonb"), ColumnKind::Json);
        assert_eq!(column_kind("bytea"), ColumnKind::Bytes);
        assert_eq!(column_kind("BLOB"), ColumnKind::Bytes);
        assert_eq!(column_kind("BINARY(16)"), ColumnKind::Bytes);
        assert_eq!(column_kind("VARBINARY(32)"), ColumnKind::Bytes);
        assert_eq!(column_kind("IMAGE"), ColumnKind::Bytes);
    }

    #[test]
    fn postgres_array_csv_cells_remain_text_instead_of_being_parsed_as_scalars() {
        let columns = vec![column("value", "float8[]")];
        let row = vec!["{\"1.0000000000000002\",\"-0\",\"5e-324\",\"NaN\",\"Infinity\",\"-Infinity\",NULL}".into()];
        let values = row_to_values(&row, &[Some(0)], &columns, &CsvImportOptions::default(), 2)
            .expect("float8[] CSV cells remain exact text");

        assert_eq!(values, vec![Value::Text(row[0].clone())]);
    }

    #[test]
    fn clickhouse_wide_integer_csv_cells_remain_exact_text() {
        let columns = vec![column("signed", "Int128"), column("unsigned", "UInt128")];
        let row = vec![
            "-170141183460469231731687303715884105728".to_owned(),
            "340282366920938463463374607431768211455".to_owned(),
        ];
        let values = row_to_values(&row, &[Some(0), Some(1)], &columns, &CsvImportOptions::default(), 2)
            .expect("wide integer values remain text");

        assert_eq!(values, row.into_iter().map(Value::Text).collect::<Vec<_>>());
    }

    #[test]
    fn formula_marker_is_removed_only_from_valid_signed_wide_integer_cells() {
        let columns = vec![
            column("signed", "Int128"),
            column("unsigned", "UInt128"),
            column("invalid_unsigned", "UInt128"),
            column("overflow", "Int128"),
            column("invalid_signed", "Int128"),
            column("text", "String"),
        ];
        let signed_min = "-170141183460469231731687303715884105728";
        let unsigned_max = "340282366920938463463374607431768211455";
        let row = vec![
            format!("'{signed_min}"),
            format!("'+{unsigned_max}"),
            "'-1".into(),
            "'170141183460469231731687303715884105728".into(),
            "'-not-an-integer".into(),
            "'-170141183460469231731687303715884105728".into(),
        ];
        let values = row_to_values(
            &row,
            &[Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)],
            &columns,
            &CsvImportOptions::default(),
            2,
        )
        .expect("valid wide integer formula prefixes are restored");

        assert_eq!(
            values,
            vec![
                Value::Text(signed_min.into()),
                Value::Text(format!("+{unsigned_max}")),
                Value::Text("'-1".into()),
                Value::Text("'170141183460469231731687303715884105728".into()),
                Value::Text("'-not-an-integer".into()),
                Value::Text("'-170141183460469231731687303715884105728".into()),
            ]
        );
    }

    #[test]
    fn value_contract_formula_safe_decimal_import_rejects_excess_precision() {
        let columns = vec![column("amount", "decimal")];
        for input in ["-0.12345678901234567890123456789", "-1.00000000000000000000000000001"] {
            for text in [input.to_owned(), format!("'{input}")] {
                let error = row_to_values(&[text], &[Some(0)], &columns, &CsvImportOptions::default(), 2)
                    .expect_err("excess precision must not be rounded");
                assert_eq!(error.reason, CellError::NotANumber);
                assert_eq!(error.line, 2);
                assert_eq!(error.column, "amount");
            }
        }
    }

    #[test]
    fn formula_marker_is_removed_from_negative_decimal_cells_only_for_decimal_columns() {
        let columns = vec![column("amount", "decimal"), column("text", "String")];
        let row = vec!["'-123.45".to_owned(), "'-123.45".to_owned()];
        let values = row_to_values(&row, &[Some(0), Some(1)], &columns, &CsvImportOptions::default(), 2)
            .expect("sanitized decimal is parsed while text keeps its formula marker");

        assert_eq!(values[0], Value::Decimal(Decimal::new(-12345, 2)));
        assert_eq!(values[1], Value::Text("'-123.45".into()));
    }

    #[test]
    fn an_unknown_type_is_read_as_text_rather_than_refused() {
        assert_eq!(column_kind("citext"), ColumnKind::Text);
        assert_eq!(column_kind("my_enum"), ColumnKind::Text);
    }

    #[test]
    fn a_field_that_does_not_fit_its_column_names_the_failure_without_its_contents() {
        let error = parse_cell("not a number", ColumnKind::Int).expect_err("refused");

        assert_eq!(error, CellError::NotAnInteger);
        assert!(!error.to_string().contains("not a number"));
    }

    #[test]
    fn a_row_error_names_the_line_and_column_and_carries_no_cell_text() {
        let columns = vec![column("id", "bigint")];

        let error = row_to_values(
            &["secret-value".to_owned()],
            &[Some(0)],
            &columns,
            &CsvImportOptions::default(),
            4,
        )
        .expect_err("refused");

        assert_eq!(error.line, 4);
        assert_eq!(error.column, "id");
        assert!(!error.to_string().contains("secret-value"));
    }

    #[test]
    fn a_mapped_field_is_parsed_into_the_columns_type() {
        let columns = vec![column("id", "bigint"), column("name", "text")];

        let values = row_to_values(
            &["7".to_owned(), "ada".to_owned()],
            &[Some(0), Some(1)],
            &columns,
            &CsvImportOptions::default(),
            2,
        )
        .expect("values");

        assert_eq!(values, vec![Value::Int(7), Value::Text("ada".to_owned())]);
    }

    #[test]
    fn an_unmapped_column_becomes_null_so_the_server_default_applies() {
        let columns = vec![column("id", "bigint"), column("name", "text")];

        let values = row_to_values(
            &["7".to_owned()],
            &[Some(0), None],
            &columns,
            &CsvImportOptions::default(),
            2,
        )
        .expect("values");

        assert_eq!(values[1], Value::Null);
    }

    #[test]
    fn an_empty_field_is_null_for_a_column_that_is_not_text() {
        let columns = vec![column("id", "bigint")];

        let values =
            row_to_values(&[String::new()], &[Some(0)], &columns, &CsvImportOptions::default(), 2).expect("values");

        assert_eq!(values[0], Value::Null);
    }

    #[test]
    fn an_empty_field_stays_an_empty_string_for_a_text_column() {
        let columns = vec![column("name", "text")];

        let values =
            row_to_values(&[String::new()], &[Some(0)], &columns, &CsvImportOptions::default(), 2).expect("values");

        assert_eq!(values[0], Value::Text(String::new()));
    }

    #[test]
    fn a_null_marker_the_user_set_is_null_even_for_text() {
        let options = CsvImportOptions {
            null_marker: "\\N".to_owned(),
            ..CsvImportOptions::default()
        };
        let columns = vec![column("name", "text")];

        let values = row_to_values(&["\\N".to_owned()], &[Some(0)], &columns, &options, 2).expect("values");

        assert_eq!(values[0], Value::Null);
    }

    #[test]
    fn the_temporal_types_parse_the_forms_an_export_writes() {
        assert!(matches!(parse_cell("2024-05-06", ColumnKind::Date), Ok(Value::Date(_))));
        assert!(matches!(parse_cell("13:45:01", ColumnKind::Time), Ok(Value::Time(_))));
        assert!(matches!(
            parse_cell("2024-05-06 13:45:01", ColumnKind::DateTime),
            Ok(Value::DateTime(_))
        ));
        assert!(matches!(
            parse_cell("2024-05-06T13:45:01Z", ColumnKind::TimestampTz),
            Ok(Value::TimestampTz(_))
        ));
    }

    #[test]
    fn booleans_read_the_spellings_the_engines_export() {
        assert_eq!(parse_cell("TRUE", ColumnKind::Bool), Ok(Value::Bool(true)));
        assert_eq!(parse_cell("f", ColumnKind::Bool), Ok(Value::Bool(false)));
        assert_eq!(parse_cell("maybe", ColumnKind::Bool), Err(CellError::NotABoolean));
    }

    #[test]
    fn binary_takes_hexadecimal_with_or_without_the_engine_prefix() {
        assert_eq!(
            parse_cell("0xDEad", ColumnKind::Bytes),
            Ok(Value::Bytes(vec![0xde, 0xad]))
        );
        assert_eq!(
            parse_cell("0XDEad", ColumnKind::Bytes),
            Ok(Value::Bytes(vec![0xde, 0xad]))
        );
        assert_eq!(
            parse_cell("beef", ColumnKind::Bytes),
            Ok(Value::Bytes(vec![0xbe, 0xef]))
        );
        assert_eq!(parse_cell("xyz", ColumnKind::Bytes), Err(CellError::NotBytes));
        assert_eq!(parse_cell("\\x", ColumnKind::Bytes), Ok(Value::Bytes(vec![])));
        assert_eq!(parse_cell("\\x00ff", ColumnKind::Bytes), Ok(Value::Bytes(vec![0, 255])));
        for text in ["\\x0", "\\xgg", "\\xé", "0x0", "0xgg"] {
            assert_eq!(parse_cell(text, ColumnKind::Bytes), Err(CellError::NotBytes));
        }
    }
}
