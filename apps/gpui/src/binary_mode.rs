use base64::Engine;
use bookie_gpui::model::{Cell, CellText};
use std::collections::HashMap;

const PREVIEW_BYTES: usize = 24;
const DETAIL_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum BinaryMode {
    #[default]
    Hex,
    Utf8,
    Base64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Scope {
    Query {
        connection: String,
        database: String,
    },
    Table {
        connection: String,
        database: String,
        schema: Option<String>,
        table: String,
    },
}

pub(crate) type ColumnModes = HashMap<(Scope, String, usize), BinaryMode>;

impl BinaryMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Hex => "Hex",
            Self::Utf8 => "UTF-8 Text",
            Self::Base64 => "Base64",
        }
    }
}

pub(crate) fn is_binary_type(data_type: &str) -> bool {
    let kind = data_type.to_ascii_lowercase();
    kind.contains("byte") || kind.contains("blob") || kind.contains("binary") || kind == "image"
}

pub(crate) fn preview(cell: &Cell, mode: BinaryMode) -> String {
    let Cell::Bytes(bytes) = cell else {
        return bookie_gpui::preview::cell_preview(cell);
    };
    match mode {
        BinaryMode::Hex => bookie_gpui::preview::cell_preview(cell),
        BinaryMode::Utf8 => bounded_text(bytes, PREVIEW_BYTES),
        BinaryMode::Base64 => bounded_base64(bytes, PREVIEW_BYTES),
    }
}

pub(crate) fn edit_text(bytes: &[u8], mode: BinaryMode) -> Result<String, &'static str> {
    match mode {
        BinaryMode::Hex => Ok(Cell::Bytes(bytes.to_vec()).display()),
        BinaryMode::Utf8 => std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| "This binary value is not valid UTF-8"),
        BinaryMode::Base64 => Ok(base64::engine::general_purpose::STANDARD.encode(bytes)),
    }
}

pub(crate) fn parse_edit(text: &str, mode: BinaryMode) -> Result<Cell, String> {
    match mode {
        BinaryMode::Hex => bookie_gpui::edit::parse_edit("bytes", text),
        BinaryMode::Utf8 => Ok(Cell::Bytes(text.as_bytes().to_vec())),
        BinaryMode::Base64 => base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .map(Cell::Bytes)
            .map_err(|_| "not valid Base64 bytes".into()),
    }
}

pub(crate) fn detail_text(bytes: &[u8], mode: BinaryMode) -> String {
    let shown = bytes.len().min(DETAIL_BYTES);
    let mut text = match mode {
        BinaryMode::Hex => return String::new(),
        BinaryMode::Utf8 => return bounded_text(bytes, DETAIL_BYTES),
        BinaryMode::Base64 => base64::engine::general_purpose::STANDARD.encode(&bytes[..shown]),
    };
    if bytes.len() > shown {
        text.push_str(&format!("… (showing {shown} of {} bytes)", bytes.len()));
    }
    text
}

fn bounded_text(bytes: &[u8], limit: usize) -> String {
    let shown = bytes.len().min(limit);
    let (text, displayed) = match std::str::from_utf8(&bytes[..shown]) {
        Ok(text) => (text.to_owned(), shown),
        Err(error) if error.error_len().is_none() && bytes.len() > shown => (
            String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned(),
            error.valid_up_to(),
        ),
        Err(_) => return format!("Invalid UTF-8 · {} bytes", bytes.len()),
    };
    if bytes.len() > displayed {
        format!("{text}… (showing {displayed} of {} bytes)", bytes.len())
    } else {
        text
    }
}

fn bounded_base64(bytes: &[u8], limit: usize) -> String {
    let shown = bytes.len().min(limit);
    let text = base64::engine::general_purpose::STANDARD.encode(&bytes[..shown]);
    if bytes.len() > shown {
        format!("{text}… ({} bytes total)", bytes.len())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::{BinaryMode, bounded_base64, bounded_text, is_binary_type, parse_edit, preview};
    use bookie_gpui::model::Cell;

    #[test]
    fn binary_modes_round_trip_without_changing_cell_type() {
        let bytes = [0, 255, 65];
        for (mode, text) in [
            (BinaryMode::Hex, "\\x00ff41"),
            (BinaryMode::Utf8, "This binary value is not valid UTF-8"),
            (BinaryMode::Base64, "AP9B"),
        ] {
            let rendered = super::edit_text(&bytes, mode);
            match mode {
                BinaryMode::Utf8 => assert_eq!(rendered, Err(text)),
                _ => assert_eq!(rendered.as_deref(), Ok(text)),
            }
        }
        assert_eq!(parse_edit("AP9B", BinaryMode::Base64), Ok(Cell::Bytes(bytes.to_vec())));
        assert_eq!(
            parse_edit("hello", BinaryMode::Utf8),
            Ok(Cell::Bytes(b"hello".to_vec()))
        );
        assert_eq!(
            parse_edit("\\x00ff41", BinaryMode::Hex),
            Ok(Cell::Bytes(bytes.to_vec()))
        );
        assert_eq!(
            parse_edit("not base64!", BinaryMode::Base64),
            Err("not valid Base64 bytes".into())
        );
    }

    #[test]
    fn previews_and_details_are_bounded_and_reject_invalid_utf8() {
        let bytes = vec![b'a'; 1_000_000];
        assert!(preview(&Cell::Bytes(bytes.clone()), BinaryMode::Base64).len() < 100);
        assert!(super::detail_text(&bytes, BinaryMode::Base64).len() < 6000);
        assert!(bounded_text(&[0xff], 24).starts_with("Invalid UTF-8"));
        assert_eq!(preview(&Cell::Bytes(b"hello".to_vec()), BinaryMode::Utf8), "hello");
        assert_eq!(bounded_base64(b"A", 24), "QQ==");
    }

    #[test]
    fn utf8_detail_does_not_silently_drop_an_incomplete_final_character() {
        let mut bytes = vec![b'a'; super::DETAIL_BYTES - 1];
        bytes.push(0xc3);
        assert!(super::detail_text(&bytes, BinaryMode::Utf8).starts_with("Invalid UTF-8"));
    }

    #[test]
    fn binary_metadata_names_cover_common_database_types() {
        for kind in ["BYTEA", "BLOB", "VARBINARY(MAX)", "IMAGE"] {
            assert!(is_binary_type(kind), "{kind}");
        }
        assert!(!is_binary_type("text"));
    }
}
