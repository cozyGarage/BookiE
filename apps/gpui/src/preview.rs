use crate::model::{Cell, CellText};

const MAX_CHARS: usize = 512;

pub fn cell_preview(cell: &Cell) -> String {
    match cell {
        Cell::Bool(true) => "TRUE".into(),
        Cell::Bool(false) => "FALSE".into(),
        Cell::Text(text) => text_preview(text),
        Cell::JsonText(text) => json_text_preview(text),
        Cell::Json(value) => json_preview(value),
        Cell::Undecodable(value) => format!("<undecodable {}>", text_preview(value)),
        Cell::Bytes(bytes) if bytes.len() > 16 => {
            let sample = Cell::Bytes(bytes[..16].to_vec()).display();
            format!("{sample}… ({} bytes total)", bytes.len())
        }
        _ => cell.display(),
    }
}

struct JsonPreview {
    bytes: Vec<u8>,
    limit: usize,
}

impl std::io::Write for JsonPreview {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let remaining = self.limit.saturating_sub(self.bytes.len());
        let take = remaining.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..take]);
        if take < bytes.len() {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        Ok(take)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn bounded_json(value: &serde_json::Value, limit: usize, pretty: bool) -> (String, bool) {
    let mut output = JsonPreview {
        bytes: Vec::with_capacity(limit),
        limit,
    };
    let result = if pretty {
        serde_json::to_writer_pretty(&mut output, value)
    } else {
        serde_json::to_writer(&mut output, value)
    };
    let end = std::str::from_utf8(&output.bytes).map_or_else(|error| error.valid_up_to(), str::len);
    output.bytes.truncate(end);
    (String::from_utf8(output.bytes).unwrap_or_default(), result.is_err())
}

fn json_preview(value: &serde_json::Value) -> String {
    let (text, truncated) = bounded_json(value, MAX_CHARS, true);
    let text = text_preview(&text.replace('\n', " "));
    if truncated { format!("{text}…") } else { text }
}

fn json_text_preview(text: &str) -> String {
    let preview = text_preview(text);
    let mut out = String::with_capacity(preview.len());
    let mut quoted = false;
    let mut escaped = false;
    let mut chars = preview.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
        } else if matches!(ch, ':' | ',') && chars.peek().is_some_and(|next| !next.is_whitespace()) {
            out.push(' ');
        }
    }
    if out.chars().count() > MAX_CHARS + 64 {
        let prefix: String = out.chars().take(MAX_CHARS).collect();
        format!("{prefix}… ({} bytes total)", text.len())
    } else {
        out
    }
}

fn text_preview(text: &str) -> String {
    let end = text
        .char_indices()
        .nth(MAX_CHARS)
        .map_or(text.len(), |(index, _)| index);
    let sample: String = text[..end]
        .chars()
        .map(|character| match character {
            '\0' => '␀',
            '\n' | '\u{2028}' | '\u{2029}' => '↵',
            '\r' => '␍',
            '\t' => '⇥',
            other => other,
        })
        .collect();
    if end == text.len() {
        return sample;
    }
    format!("{sample}… ({} bytes total)", text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_cells_have_readable_previews_without_changing_values() {
        assert_eq!(cell_preview(&Cell::Null), Cell::Null.display());
        assert_eq!(cell_preview(&Cell::Bool(true)), "TRUE");
        assert_eq!(cell_preview(&Cell::Bool(false)), "FALSE");
        assert_eq!(cell_preview(&Cell::Text("true".into())), "true");
        let raw = Cell::JsonText(r#"{"a":"x,y:z","a":2}"#.into());
        assert_eq!(cell_preview(&raw), r#"{"a": "x,y:z", "a": 2}"#);
        assert_eq!(raw.display(), r#"{"a":"x,y:z","a":2}"#);
        let json = Cell::Json(Box::new(serde_json::json!({"outer": {"n": 1}})));
        let preview = cell_preview(&json);
        assert!(preview.contains("\"outer\": "));
        assert!(preview.contains("\"n\": 1"));
        assert_eq!(json, Cell::Json(Box::new(serde_json::json!({"outer": {"n": 1}}))));
    }

    #[test]
    fn json_text_spacing_handles_escapes_and_stays_bounded() {
        let raw = r#"{"a":"escaped\" ,: text","b":1}"#;
        assert_eq!(
            cell_preview(&Cell::JsonText(raw.into())),
            r#"{"a": "escaped\" ,: text", "b": 1}"#
        );
        let long = format!("[{}]", "1,".repeat(2000));
        let shown = cell_preview(&Cell::JsonText(long.clone()));
        assert!(shown.chars().count() <= MAX_CHARS + 64);
        assert!(shown.ends_with(&format!("({} bytes total)", long.len())));
    }

    #[test]
    fn real_json_is_shared_and_previewed_without_losing_its_value() {
        let value = bookie_core::Value::Json(Box::new(serde_json::json!({"text": "💾".repeat(100_000)})));
        let cell = value.clone();
        let copy = cell.clone();
        assert_eq!(copy, value);
        let preview = cell_preview(&cell);
        assert!(preview.len() <= MAX_CHARS + '…'.len_utf8());
        assert!(preview.ends_with('…'));
        assert_eq!(cell, value);
    }

    #[test]
    fn real_json_preview_flattens_unicode_line_separators() {
        let cell = bookie_core::Value::Json(Box::new(serde_json::json!({
            "text": "first\u{2028}second"
        })));
        let shown = cell_preview(&cell);
        assert!(shown.contains('↵'));
        assert!(!shown.contains('\u{2028}'));
    }

    #[test]
    fn oversized_unicode_and_json_keep_exact_values_outside_the_preview() {
        let text = format!("{}\n\0tail", "💾".repeat(1_000_000));
        for cell in [Cell::Text(text.clone()), Cell::JsonText(text.clone())] {
            let shown = cell_preview(&cell);
            assert!(shown.starts_with(&"💾".repeat(MAX_CHARS)));
            assert!(shown.len() < MAX_CHARS * 4 + 64);
            assert!(shown.ends_with(&format!("({} bytes total)", text.len())));
            assert_eq!(cell.display(), text);
        }
    }

    #[test]
    fn byte_preview_bounds_hex_conversion_and_keeps_the_exact_byte_count() {
        let cell = Cell::Bytes(vec![0xab; 1_000_000]);
        assert_eq!(
            cell_preview(&cell),
            format!("\\x{}… (1000000 bytes total)", "ab".repeat(16))
        );
        assert_eq!(cell.display().len(), 2_000_002);
    }

    #[test]
    fn preview_keeps_short_values_and_flattens_multiline_text() {
        assert_eq!(cell_preview(&Cell::Text("a\n\r\t\0b".into())), "a↵␍⇥␀b");
        assert_eq!(cell_preview(&Cell::Int(9_007_199_254_740_993)), "9007199254740993");
        assert_eq!(cell_preview(&Cell::Null), "NULL");
    }

    #[test]
    fn shared_large_cells_keep_exact_values_across_preview_and_core_conversion() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!("../testdata/large-cells.json")).unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let value = format!(
                "{}{}{}",
                fixture["prefix"].as_str().unwrap(),
                fixture["unit"]
                    .as_str()
                    .unwrap()
                    .repeat(fixture["repeats"].as_u64().unwrap() as usize),
                fixture["suffix"].as_str().unwrap()
            );
            let cell = crate::edit::parse_edit(fixture["kind"].as_str().unwrap(), &value).unwrap();
            let shown = cell_preview(&cell);
            assert!(shown.chars().count() <= 576);
            assert!(shown.contains('…'));
            assert!(!shown.contains(['\n', '\r', '\t', '\0']));
            assert_eq!(cell.display(), value);
        }
    }
}
