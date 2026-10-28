use tablepro_core::{QueryResult, Value};

use super::BrowseTarget;

const LONG_TEXT_BYTES: usize = 1_048_576;
const PREVIEW_CHARS: usize = 1024;
const MARKER: &str = "bookie-partial:";

pub(super) fn projection(target: &BrowseTarget<'_>) -> Option<String> {
    if !target.long_text_preview {
        return None;
    }
    let driver_id = target.driver_id;
    let quote = |name: &str| tablepro_core::sql_dialect::quote_ident(driver_id, name);
    let mut any = false;
    let list: Vec<String> = target
        .columns
        .iter()
        .map(|column| {
            let name = quote(&column.name);
            match preview_expression(driver_id, &column.data_type, &name) {
                Some(expression) => {
                    any = true;
                    format!("{expression} AS {name}")
                }
                None => name,
            }
        })
        .collect();
    any.then(|| list.join(", "))
}

fn preview_expression(driver_id: &str, data_type: &str, name: &str) -> Option<String> {
    let lower = data_type.trim().to_ascii_lowercase();
    let limit = LONG_TEXT_BYTES;
    let prefix = PREVIEW_CHARS;
    match driver_id {
        "postgres" if lower == "text" || lower == "character varying" => Some(format!(
            "CASE WHEN octet_length({name}) > {limit} THEN E'\\x01{MARKER}' || octet_length({name})::text \
             || E'\\x01' || left({name}, {prefix}) ELSE {name} END"
        )),
        "mysql" if lower == "mediumtext" || lower == "longtext" => Some(format!(
            "CASE WHEN OCTET_LENGTH({name}) > {limit} THEN CONCAT(CHAR(1 USING utf8mb4), '{MARKER}', \
             OCTET_LENGTH({name}), CHAR(1 USING utf8mb4), LEFT({name}, {prefix})) ELSE {name} END"
        )),
        "sqlite" if ["text", "clob", "char"].iter().any(|kind| lower.contains(kind)) => Some(format!(
            "CASE WHEN length(CAST({name} AS BLOB)) > {limit} THEN char(1) || '{MARKER}' \
             || length(CAST({name} AS BLOB)) || char(1) || substr({name}, 1, {prefix}) ELSE {name} END"
        )),
        "mssql" if lower == "varchar(max)" || lower == "nvarchar(max)" => Some(format!(
            "CASE WHEN DATALENGTH({name}) > {limit} THEN NCHAR(1) + N'{MARKER}' \
             + CAST(DATALENGTH({name}) AS nvarchar(20)) + NCHAR(1) + CAST(LEFT({name}, {prefix}) AS nvarchar({prefix})) \
             ELSE {name} END"
        )),
        _ => None,
    }
}

pub fn mark_partial_cells(result: &mut QueryResult) {
    for row in &mut result.rows {
        for cell in row {
            let Value::Text(text) = cell else {
                continue;
            };
            let Some(rest) = text.strip_prefix('\u{1}').and_then(|rest| rest.strip_prefix(MARKER)) else {
                continue;
            };
            let Some((bytes, prefix)) = rest.split_once('\u{1}') else {
                continue;
            };
            if bytes.parse::<u64>().is_ok() {
                *cell = Value::Undecodable(format!("partial {bytes} bytes\u{1}{prefix}"));
            }
        }
    }
}

pub fn parse_partial(type_name: &str) -> Option<(u64, &str)> {
    let rest = type_name.strip_prefix("partial ")?;
    let (bytes, prefix) = rest.split_once(" bytes\u{1}")?;
    Some((bytes.parse().ok()?, prefix))
}
