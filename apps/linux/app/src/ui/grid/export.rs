//! All result surfaces share the core serializers.
pub(super) use tablepro_core::export::{render_in_clause, render_json, render_markdown, render_row_json, render_tsv};

pub(super) fn render_csv(
    columns: &[tablepro_core::ColumnInfo],
    rows: &[Vec<tablepro_core::Value>],
    with_headers: bool,
) -> String {
    let null_marker = tablepro_core::export::unique_csv_null_marker(rows);
    tablepro_core::export::render_csv(
        columns,
        rows,
        &tablepro_core::export::CsvOptions {
            header_row: with_headers,
            null_to_empty: false,
            null_marker: Some(null_marker),
            ..Default::default()
        },
    )
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
