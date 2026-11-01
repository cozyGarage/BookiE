//! All result surfaces share the core serializers.
pub(super) use bookie_core::export::{
    render_in_clause, render_json, render_json_lines, render_markdown, render_row_json, render_text_table, render_tsv,
    render_value_list,
};

pub(super) fn render_csv(
    columns: &[bookie_core::ColumnInfo],
    rows: &[Vec<bookie_core::Value>],
    with_headers: bool,
) -> String {
    let null_marker = bookie_core::export::unique_csv_null_marker(rows);
    bookie_core::export::render_csv(
        columns,
        rows,
        &bookie_core::export::CsvOptions {
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
