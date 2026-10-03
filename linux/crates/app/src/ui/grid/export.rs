//! All result surfaces share the core serializers.
pub(super) use tablepro_core::export::{render_in_clause, render_json, render_markdown, render_tsv, row_to_json};

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
mod tests {
    use tablepro_core::{ColumnInfo, Value};

    use super::render_csv;

    #[test]
    fn clipboard_csv_keeps_null_empty_enum_and_marker_labels_distinct() {
        let columns = vec![ColumnInfo {
            name: "status".into(),
            data_type: "sample_schema.status_kind".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: Some(tablepro_core::QualifiedTypeName {
                schema: "sample_schema".into(),
                name: "status_kind".into(),
            }),
        }];
        let rows = vec![
            vec![Value::Text("NULL".into())],
            vec![Value::Text(String::new())],
            vec![Value::Null],
            vec![Value::Text("\\N".into())],
            vec![Value::Text("\\NN".into())],
            vec![Value::Text("=1+1".into())],
        ];

        assert_eq!(
            render_csv(&columns, &rows, false),
            "NULL\n\"\"\n\\NNN\n\\N\n\\NN\n\"'=1+1\"\n"
        );
        assert_eq!(
            render_csv(&columns, &rows, true),
            "status\nNULL\n\"\"\n\\NNN\n\\N\n\\NN\n\"'=1+1\"\n"
        );
    }
}
