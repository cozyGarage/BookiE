//! All result surfaces share the core serializers.
pub(super) use tablepro_core::export::{render_in_clause, render_json, render_markdown, render_tsv, row_to_json};

pub(super) fn render_csv(
    columns: &[tablepro_core::ColumnInfo],
    rows: &[Vec<tablepro_core::Value>],
    with_headers: bool,
) -> String {
    tablepro_core::export::render_csv(
        columns,
        rows,
        &tablepro_core::export::CsvOptions {
            header_row: with_headers,
            ..Default::default()
        },
    )
}
