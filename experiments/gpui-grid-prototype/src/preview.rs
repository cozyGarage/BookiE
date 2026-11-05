use crate::model::Cell;

const MAX_CHARS: usize = 512;

pub fn cell_preview(cell: &Cell) -> String {
    match cell {
        Cell::Text(text)
        | Cell::Json(text)
        | Cell::JsonText(text)
        | Cell::Date(text)
        | Cell::Time(text)
        | Cell::DateTime(text)
        | Cell::TimestampTz(text)
        | Cell::Decimal(text)
        | Cell::Uuid(text) => text_preview(text),
        Cell::Undecodable(name) => format!("<undecodable {}>", text_preview(name)),
        Cell::Bytes(bytes) if bytes.len() > 16 => {
            let sample = Cell::Bytes(bytes[..16].to_vec()).display();
            format!("{sample}… ({} bytes total)", bytes.len())
        }
        _ => cell.display(),
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
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../fixtures/large-cells.json")).unwrap();
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
            assert_eq!(
                crate::core_bridge::value_to_cell(crate::core_bridge::cell_to_value(&cell)),
                cell
            );
        }
    }
}
