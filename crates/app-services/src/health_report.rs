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

#[cfg(test)]
#[path = "health_report_tests.rs"]
mod tests;
