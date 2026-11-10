use crate::model::Cell;

pub const HEX_BYTES_PER_LINE: usize = 16;
pub const MAX_HEX_BYTES: usize = 4096;
pub const MAX_PRETTY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexLine {
    pub offset: usize,
    pub hex: String,
    pub ascii: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    Null,
    Hex {
        lines: Vec<HexLine>,
        total: usize,
        shown: usize,
    },
    Json {
        pretty: String,
    },
    Text(String),
}

pub fn hex_lines(bytes: &[u8]) -> Vec<HexLine> {
    bytes
        .chunks(HEX_BYTES_PER_LINE)
        .take(MAX_HEX_BYTES / HEX_BYTES_PER_LINE)
        .enumerate()
        .map(|(line, chunk)| HexLine {
            offset: line * HEX_BYTES_PER_LINE,
            hex: chunk
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" "),
            ascii: chunk
                .iter()
                .map(|byte| {
                    if byte.is_ascii_graphic() || *byte == b' ' {
                        char::from(*byte)
                    } else {
                        '.'
                    }
                })
                .collect(),
        })
        .collect()
}

pub fn detail_of(cell: &Cell) -> Detail {
    match cell {
        Cell::Null => Detail::Null,
        Cell::Bytes(bytes) => Detail::Hex {
            lines: hex_lines(bytes),
            total: bytes.len(),
            shown: bytes.len().min(MAX_HEX_BYTES),
        },
        Cell::Json(text) | Cell::JsonText(text) if text.len() <= MAX_PRETTY_BYTES => {
            match serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .and_then(|value| serde_json::to_string_pretty(&value).ok())
            {
                Some(pretty) => Detail::Json { pretty },
                None => Detail::Text(text.clone()),
            }
        }
        other => Detail::Text(other.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_become_offset_hex_and_ascii_lines_sixteen_wide() {
        let bytes: Vec<u8> = (0..20).collect();
        let lines = hex_lines(&bytes);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].offset, 0);
        assert_eq!(lines[0].hex, "00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f");
        assert_eq!(lines[1].offset, 16);
        assert_eq!(lines[1].hex, "10 11 12 13");
        assert_eq!(lines[1].ascii, "....");
        let text = hex_lines(b"Hi, \xffA");
        assert_eq!(text[0].ascii, "Hi, .A");
    }

    #[test]
    fn a_large_blob_is_cut_at_the_cap_and_reports_its_full_size() {
        let blob = Cell::Bytes(vec![7; MAX_HEX_BYTES + 100]);
        let Detail::Hex { lines, total, shown } = detail_of(&blob) else {
            panic!("bytes show as hex");
        };
        assert_eq!((total, shown), (MAX_HEX_BYTES + 100, MAX_HEX_BYTES));
        assert_eq!(lines.len(), MAX_HEX_BYTES / HEX_BYTES_PER_LINE);
    }

    #[test]
    fn json_is_pretty_printed_and_invalid_json_stays_raw() {
        let Detail::Json { pretty } = detail_of(&Cell::Json(r#"{"a":[1,{"b":null}]}"#.into())) else {
            panic!("valid json is pretty printed");
        };
        assert!(pretty.contains("\n  \"a\": ["), "{pretty}");
        assert_eq!(
            detail_of(&Cell::JsonText("{broken".into())),
            Detail::Text("{broken".into())
        );
    }

    #[test]
    fn null_and_plain_cells_have_their_own_detail() {
        assert_eq!(detail_of(&Cell::Null), Detail::Null);
        assert_eq!(detail_of(&Cell::Int(42)), Detail::Text("42".into()));
        assert_eq!(
            detail_of(&Cell::Text("line one\nline two".into())),
            Detail::Text("line one\nline two".into())
        );
    }
}
