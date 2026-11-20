use bookie_core::{Connection, DriverError, HealthReport, OperationControl, QueryResult, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthRow {
    pub title: String,
    pub summary: String,
    pub diagnostic_sql: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthRows {
    pub rows: Vec<HealthRow>,
    pub may_have_more: bool,
}

pub async fn run_health_report(
    connection: &dyn Connection,
    report: HealthReport,
    hidden_schemas: &[String],
    control: &OperationControl,
) -> Result<HealthRows, DriverError> {
    let params = [bookie_core::hidden_schemas_param(hidden_schemas)];
    let result = connection
        .query_params_controlled(&report.sql(), &params, control)
        .await?;
    Ok(rows_of(report, &result))
}

fn rows_of(report: HealthReport, result: &QueryResult) -> HealthRows {
    let names: Vec<&str> = result.columns.iter().map(|column| column.name.as_str()).collect();
    let rows: Vec<HealthRow> = result.rows.iter().map(|cells| row_of(report, &names, cells)).collect();
    HealthRows {
        may_have_more: rows.len() >= bookie_core::HEALTH_ROW_LIMIT,
        rows,
    }
}

fn row_of(report: HealthReport, names: &[&str], cells: &[Value]) -> HealthRow {
    let text_of = |wanted: &str| {
        let index = names.iter().position(|name| *name == wanted)?;
        match cells.get(index)? {
            Value::Text(text) => Some(text.as_str()),
            _ => None,
        }
    };
    let schema = text_of("schema_name").unwrap_or_default();
    let object_column = if names.contains(&"index_name") {
        "index_name"
    } else {
        "table_name"
    };
    let object = text_of(object_column).unwrap_or_default();
    let summary = names
        .iter()
        .zip(cells)
        .filter(|(name, _)| shown_in_summary(name, object_column))
        .map(|(name, value)| format!("{}: {}", column_label(name), cell_text(value)))
        .collect::<Vec<_>>()
        .join("  ");
    HealthRow {
        title: format!("{schema}.{object}"),
        summary,
        diagnostic_sql: report.diagnostic_sql(schema, object),
    }
}

fn shown_in_summary(name: &str, object_column: &str) -> bool {
    name != "schema_name" && name != object_column && !name.ends_with("_bytes")
}

fn cell_text(value: &Value) -> String {
    match value {
        Value::Null => crate::tr!("none"),
        other => crate::value_display::copy_text(other).into_string(),
    }
}

fn column_label(name: &str) -> String {
    match name {
        "table_name" => crate::tr!("Table"),
        "total_size" | "index_size" => crate::tr!("Size"),
        "live_rows" => crate::tr!("Live rows"),
        "dead_rows" => crate::tr!("Dead rows"),
        "dead_percent" => crate::tr!("Dead rows %"),
        "last_vacuum" => crate::tr!("Last vacuum"),
        "last_analyze" => crate::tr!("Last analyze"),
        "estimated_bloat" => crate::tr!("Estimated bloat (a rough estimate)"),
        "scans" => crate::tr!("Index scans"),
        "statistics_since" => crate::tr!("Statistics since"),
        "covered_by" => crate::tr!("Covered by"),
        "definition" => crate::tr!("Definition"),
        "seq_scans" => crate::tr!("Sequential scans"),
        "index_scans" => crate::tr!("Index scans"),
        "seq_scan_percent" => crate::tr!("Sequential scan %"),
        "rows_read_by_seq_scans" => crate::tr!("Rows read by sequential scans"),
        other => other.to_owned(),
    }
}

pub fn report_label(report: HealthReport) -> String {
    match report {
        HealthReport::TableVacuum => crate::tr!("Dead rows and vacuum"),
        HealthReport::EstimatedBloat => crate::tr!("Estimated bloat"),
        HealthReport::UnusedIndexes => crate::tr!("Unused indexes"),
        HealthReport::DuplicateIndexes => crate::tr!("Duplicate indexes"),
        HealthReport::SeqScanHeavy => crate::tr!("Sequential scans"),
    }
}

pub fn report_hint(report: HealthReport) -> String {
    match report {
        HealthReport::TableVacuum => {
            crate::tr!("Tables with the most dead rows, with their size and last vacuum and analyze.")
        }
        HealthReport::EstimatedBloat => {
            crate::tr!(
                "A rough estimate from the share of dead rows in tables of 1 MiB or more. It is not a measurement."
            )
        }
        HealthReport::UnusedIndexes => crate::tr!(
            "Non-unique indexes never scanned since the statistics were last reset. Check how long the statistics have run before dropping one."
        ),
        HealthReport::DuplicateIndexes => {
            crate::tr!("Indexes whose leading columns are already covered by another index on the same table.")
        }
        HealthReport::SeqScanHeavy => {
            crate::tr!("Tables of 10,000 rows or more that are read by sequential scans more often than by an index.")
        }
    }
}

pub fn rows_status(report: HealthReport, rows: &HealthRows) -> String {
    if rows.rows.is_empty() {
        return crate::tr!("Nothing to report. {hint}").replace("{hint}", &report_hint(report));
    }
    let shown = crate::tr!("{count} rows. {hint}")
        .replace("{count}", &rows.rows.len().to_string())
        .replace("{hint}", &report_hint(report));
    if rows.may_have_more {
        return format!("{shown} {}", crate::tr!("Only the top rows are shown."));
    }
    shown
}

#[cfg(test)]
#[path = "health_report_tests.rs"]
mod tests;
