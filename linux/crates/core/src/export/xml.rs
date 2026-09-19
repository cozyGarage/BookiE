use std::io::{self, Write};

use super::file::ResultWriter;
use super::value_to_text;
use crate::query::{ColumnInfo, Value};

const REPLACEMENT: char = '\u{fffd}';

pub(crate) struct XmlWriter {
    elements: Vec<String>,
}

impl XmlWriter {
    pub(crate) fn new() -> Self {
        Self { elements: Vec::new() }
    }
}

impl ResultWriter for XmlWriter {
    fn begin(&mut self, output: &mut dyn Write, columns: &[ColumnInfo]) -> io::Result<()> {
        self.elements = columns.iter().map(|column| element_name(&column.name)).collect();
        output.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rows>\n")
    }

    fn write_row(&mut self, output: &mut dyn Write, _index: usize, row: &[Value]) -> io::Result<()> {
        output.write_all(b"  <row>\n")?;
        for (index, value) in row.iter().enumerate() {
            let element = match self.elements.get(index) {
                Some(element) => element.as_str(),
                None => "column",
            };
            match value_to_text(value) {
                Some(text) => writeln!(output, "    <{element}>{}</{element}>", escape_xml_text(&text))?,
                None => writeln!(output, "    <{element} null=\"true\"/>")?,
            }
        }
        output.write_all(b"  </row>\n")
    }

    fn finish(&mut self, output: &mut dyn Write) -> io::Result<()> {
        output.write_all(b"</rows>\n")
    }
}

/// XML 1.0 `Char` excludes every control character except tab, line feed
/// and carriage return, and also excludes U+FFFE and U+FFFF. A parser
/// rejects the whole document when one appears, even escaped, so an
/// illegal character is replaced rather than encoded.
fn is_legal_xml_char(character: char) -> bool {
    matches!(character, '\t' | '\n' | '\r')
        || matches!(character, ' '..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn escape_xml_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            _ if !is_legal_xml_char(character) => escaped.push(REPLACEMENT),
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// XML 1.0 `Name` must start with a letter or `_`, may continue with
/// letters, digits, `_`, `-` and `.`, and may not start with any casing of
/// `xml`, which the specification reserves. A column name is arbitrary
/// database text, so every character outside that set becomes `_` and a
/// name that cannot start a `Name` is prefixed with one. The colon is
/// excluded because it declares a namespace prefix.
fn element_name(column: &str) -> String {
    let mut name = String::with_capacity(column.len() + 1);
    for character in column.chars() {
        if is_name_char(character) {
            name.push(character);
        } else {
            name.push('_');
        }
    }
    let starts_badly = match name.chars().next() {
        Some(first) => !is_name_start_char(first),
        None => true,
    };
    if starts_badly || starts_with_reserved_prefix(&name) {
        name.insert(0, '_');
    }
    name
}

fn starts_with_reserved_prefix(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 3
        && bytes[0].eq_ignore_ascii_case(&b'x')
        && bytes[1].eq_ignore_ascii_case(&b'm')
        && bytes[2].eq_ignore_ascii_case(&b'l')
}

fn is_name_start_char(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

fn is_name_char(character: char) -> bool {
    is_name_start_char(character) || character == '-' || character == '.' || character.is_numeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::test_support::column;

    fn render(columns: &[ColumnInfo], rows: &[Vec<Value>]) -> String {
        let mut writer = XmlWriter::new();
        let mut output: Vec<u8> = Vec::new();
        writer.begin(&mut output, columns).unwrap();
        for (index, row) in rows.iter().enumerate() {
            writer.write_row(&mut output, index, row).unwrap();
        }
        writer.finish(&mut output).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn a_column_name_that_is_not_a_valid_element_name_is_sanitised() {
        assert_eq!(element_name("2 bad name!"), "_2_bad_name_");
        assert_eq!(element_name(""), "_");
        assert_eq!(element_name("xmlns"), "_xmlns");
        assert_eq!(element_name("XmlThing"), "_XmlThing");
        assert_eq!(element_name("ok.name-2"), "ok.name-2");
        assert_eq!(element_name("ns:name"), "ns_name");
    }

    #[test]
    fn a_sanitised_column_name_is_used_for_every_row_element() {
        let xml = render(&[column("2 bad name!")], &[vec![Value::Int(1)]]);
        assert!(xml.contains("<_2_bad_name_>1</_2_bad_name_>"), "{xml}");
    }

    #[test]
    fn a_control_character_a_parser_would_reject_is_replaced_not_encoded() {
        let xml = render(&[column("value")], &[vec![Value::Text("a\u{0}b\u{1b}c\td".into())]]);
        assert!(xml.contains("a\u{fffd}b\u{fffd}c\td"), "{xml}");
        assert!(!xml.contains('\u{0}'), "{xml}");
    }

    #[test]
    fn a_cell_cannot_close_its_element_or_open_another() {
        let xml = render(
            &[column("value")],
            &[vec![Value::Text("</value><injected a=\"1\">&'".into())]],
        );
        assert!(
            xml.contains("<value>&lt;/value&gt;&lt;injected a=&quot;1&quot;&gt;&amp;&apos;</value>"),
            "{xml}"
        );
    }

    #[test]
    fn a_null_cell_is_distinguishable_from_an_empty_string() {
        let xml = render(
            &[column("value")],
            &[vec![Value::Null], vec![Value::Text(String::new())]],
        );
        assert!(xml.contains("<value null=\"true\"/>"), "{xml}");
        assert!(xml.contains("<value></value>"), "{xml}");
    }
}
