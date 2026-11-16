use crate::model::{Cell, CellText};

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
        decoded: Option<(&'static str, String)>,
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

fn decoded_preview(bytes: &[u8]) -> Option<(&'static str, String)> {
    use bookie_app_services::decoded_preview::{DecodedFormat, decoded_preview};
    let preview = decoded_preview(bytes)?;
    let label = match preview.format {
        DecodedFormat::MessagePack => "MessagePack",
        DecodedFormat::Protobuf => "Possible Protobuf (no schema, field numbers only)",
    };
    Some((label, preview.text))
}

fn hex_detail(bytes: &[u8]) -> Detail {
    Detail::Hex {
        lines: hex_lines(bytes),
        total: bytes.len(),
        shown: bytes.len().min(MAX_HEX_BYTES),
        decoded: decoded_preview(bytes),
    }
}

pub fn detail_of(cell: &Cell) -> Detail {
    match cell {
        Cell::Null => Detail::Null,
        Cell::Bytes(bytes) => hex_detail(bytes),
        Cell::Json(value) => {
            let (text, truncated) = crate::preview::bounded_json(value, MAX_PRETTY_BYTES, true);
            if truncated {
                Detail::Text(format!("{text}…"))
            } else {
                Detail::Json { pretty: text }
            }
        }
        Cell::JsonText(text) if text.len() <= MAX_PRETTY_BYTES => {
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
    fn retained_json_has_bounded_detail_and_keeps_its_value() {
        let value = bookie_core::Value::Json(Box::new(serde_json::json!({"text": "💾".repeat(100_000)})));
        let cell = value.clone();
        let Detail::Text(text) = detail_of(&cell) else {
            panic!("large JSON is bounded")
        };
        assert!(text.len() <= MAX_PRETTY_BYTES + '…'.len_utf8());
        assert!(text.ends_with('…'));
        assert_eq!(cell, value);
        let small = bookie_core::Value::Json(Box::new(serde_json::json!({"n": 1})));
        assert!(matches!(detail_of(&small), Detail::Json { .. }));
    }

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
        let Detail::Hex {
            lines, total, shown, ..
        } = detail_of(&blob)
        else {
            panic!("bytes show as hex");
        };
        assert_eq!((total, shown), (MAX_HEX_BYTES + 100, MAX_HEX_BYTES));
        assert_eq!(lines.len(), MAX_HEX_BYTES / HEX_BYTES_PER_LINE);
    }

    #[test]
    fn messagepack_bytes_get_a_decoded_preview_and_other_bytes_stay_hex_only() {
        let packed = Cell::Bytes(vec![0x82, 0xa1, b'a', 0x01, 0xa1, b'b', 0x92, 0xc3, 0xc0]);
        let Detail::Hex { decoded, lines, .. } = detail_of(&packed) else {
            panic!("bytes show as hex");
        };
        let (label, decoded) = decoded.expect("a messagepack map decodes");
        assert_eq!(label, "MessagePack");
        assert!(decoded.contains("\"a\": 1") && decoded.contains("true"), "{decoded}");
        assert_eq!(
            lines[0].hex, "82 a1 61 01 a1 62 92 c3 c0",
            "the hex dump stays alongside"
        );
        let Detail::Hex { decoded, .. } = detail_of(&Cell::Bytes(vec![0xde, 0xad, 0xbe, 0xef])) else {
            panic!("bytes show as hex");
        };
        assert_eq!(decoded, None);
    }

    #[test]
    fn protobuf_is_offered_only_as_a_labelled_guess_and_never_for_plain_text() {
        let message = Cell::Bytes(vec![
            0x08, 0x96, 0x01, 0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g',
        ]);
        let Detail::Hex { decoded, .. } = detail_of(&message) else {
            panic!("bytes show as hex");
        };
        let (label, text) = decoded.expect("two fields decode");
        assert!(label.starts_with("Possible Protobuf"), "{label}");
        assert!(text.contains("\"1\": 150") && text.contains("testing"), "{text}");
        let text_bytes = Cell::Bytes(b"hello world".to_vec());
        let Detail::Hex { decoded, .. } = detail_of(&text_bytes) else {
            panic!("bytes show as hex");
        };
        assert_eq!(decoded, None, "printable text is not Protobuf");
        let one_field = Cell::Bytes(vec![0x08, 0x01]);
        let Detail::Hex { decoded, .. } = detail_of(&one_field) else {
            panic!("bytes show as hex");
        };
        assert_eq!(decoded, None, "a single field is too weak a signal");
    }

    #[test]
    fn json_is_pretty_printed_and_invalid_json_stays_raw() {
        let Detail::Json { pretty } = detail_of(&Cell::Json(Box::new(serde_json::json!({"a": [1, {"b": null}]}))))
        else {
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
