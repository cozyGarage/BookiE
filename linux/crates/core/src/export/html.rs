use std::io::Write;

use super::error::ExportError;
use super::file::ResultWriter;
use super::value_to_text;
use crate::query::{ColumnInfo, Value};

pub(crate) struct HtmlWriter;

impl ResultWriter for HtmlWriter {
    fn begin(&mut self, output: &mut dyn Write, columns: &[ColumnInfo]) -> Result<(), ExportError> {
        output.write_all(
            b"<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>Exported rows</title>\n</head>\n<body>\n<table>\n<thead>\n<tr>",
        )?;
        for column in columns {
            write!(output, "<th>{}</th>", escape_html(&column.name))?;
        }
        output.write_all(b"</tr>\n</thead>\n<tbody>\n")?;
        Ok(())
    }

    fn write_row(&mut self, output: &mut dyn Write, _index: usize, row: &[Value]) -> Result<(), ExportError> {
        output.write_all(b"<tr>")?;
        for value in row {
            match value_to_text(value) {
                Some(text) => write!(output, "<td>{}</td>", escape_html(&text))?,
                None => output.write_all(b"<td class=\"null\"></td>")?,
            }
        }
        output.write_all(b"</tr>\n")?;
        Ok(())
    }

    fn finish(&mut self, output: &mut dyn Write) -> Result<(), ExportError> {
        output.write_all(b"</tbody>\n</table>\n</body>\n</html>\n")?;
        Ok(())
    }
}

/// HTML5 named character references. A raw apostrophe or quotation mark is
/// harmless in element content but ends an attribute value, so both are
/// escaped here rather than only where an attribute is written.
fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
        let mut writer = HtmlWriter;
        let mut output: Vec<u8> = Vec::new();
        writer.begin(&mut output, columns).unwrap();
        for (index, row) in rows.iter().enumerate() {
            writer.write_row(&mut output, index, row).unwrap();
        }
        writer.finish(&mut output).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn a_cell_cannot_inject_markup_into_the_exported_page() {
        let columns = [super::super::test_support::column("<script>")];
        let rows = vec![vec![Value::Text("<img src=x onerror=\"alert('x')\">".into())]];

        let html = render(&columns, &rows);

        assert!(html.contains("<th>&lt;script&gt;</th>"), "{html}");
        assert!(
            html.contains("<td>&lt;img src=x onerror=&quot;alert(&#39;x&#39;)&quot;&gt;</td>"),
            "{html}"
        );
        assert!(!html.contains("<script>"), "{html}");
    }

    #[test]
    fn an_ampersand_is_escaped_once_and_a_null_cell_is_marked() {
        let columns = [super::super::test_support::column("a&b")];
        let rows = vec![vec![Value::Text("x & y".into())], vec![Value::Null]];

        let html = render(&columns, &rows);

        assert!(html.contains("<th>a&amp;b</th>"), "{html}");
        assert!(html.contains("<td>x &amp; y</td>"), "{html}");
        assert!(html.contains("<td class=\"null\"></td>"), "{html}");
    }
}
