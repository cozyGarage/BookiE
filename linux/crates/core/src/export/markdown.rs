use std::io::Write;

use super::error::ExportError;
use super::file::ResultWriter;
use super::value_to_text;
use crate::query::{ColumnInfo, Value};

pub fn render_markdown(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
    let mut lines = Vec::with_capacity(rows.len() + 2);
    lines.push(markdown_header_line(columns));
    lines.push(markdown_separator_line(columns.len()));
    for row in rows {
        lines.push(markdown_row_line(row));
    }
    lines.join("\n")
}

pub(crate) fn markdown_header_line(columns: &[ColumnInfo]) -> String {
    format!(
        "| {} |",
        columns
            .iter()
            .map(|column| markdown_cell(&column.name))
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

pub(crate) fn markdown_separator_line(columns: usize) -> String {
    format!("| {} |", vec!["---"; columns].join(" | "))
}

pub(crate) fn markdown_row_line(row: &[Value]) -> String {
    format!(
        "| {} |",
        row.iter().map(markdown_value_cell).collect::<Vec<_>>().join(" | ")
    )
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
}

fn markdown_value_cell(value: &Value) -> String {
    let text = match value_to_text(value) {
        Some(text) => text,
        None => "NULL".to_string(),
    };
    markdown_cell(&text)
}

pub(crate) struct MarkdownWriter {
    columns: usize,
}

impl MarkdownWriter {
    pub(crate) fn new() -> Self {
        Self { columns: 0 }
    }
}

impl ResultWriter for MarkdownWriter {
    fn begin(&mut self, output: &mut dyn Write, columns: &[ColumnInfo]) -> Result<(), ExportError> {
        self.columns = columns.len();
        writeln!(output, "{}", markdown_header_line(columns))?;
        writeln!(output, "{}", markdown_separator_line(self.columns))?;
        Ok(())
    }

    fn write_row(&mut self, output: &mut dyn Write, _index: usize, row: &[Value]) -> Result<(), ExportError> {
        writeln!(output, "{}", markdown_row_line(row))?;
        Ok(())
    }

    fn finish(&mut self, _output: &mut dyn Write) -> Result<(), ExportError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::test_support::column;

    #[test]
    fn markdown_renderer_escapes_pipes_and_preserves_line_breaks() {
        let columns = vec![column("a|b"), column("empty")];
        let rows = vec![vec![Value::Text("first\r\nsecond|third".into()), Value::Null]];

        assert_eq!(
            render_markdown(&columns, &rows),
            "| a\\|b | empty |\n| --- | --- |\n| first<br>second\\|third | NULL |"
        );
    }
}
