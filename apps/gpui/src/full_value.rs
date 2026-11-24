use crate::model::Cell;

pub const MAX_EDIT_BYTES: usize = 1024 * 1024;

pub fn json_fits(value: &serde_json::Value) -> bool {
    struct Limit(usize);
    impl std::io::Write for Limit {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("size overflow"))?;
            if self.0 > MAX_EDIT_BYTES {
                return Err(std::io::Error::other("value too large"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Limit(0), value).is_ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Text,
    Json,
    JsonText,
    JsonFallbackText,
}

pub fn payload(kind: Kind, text: &str) -> Result<Cell, &'static str> {
    if text.len() > MAX_EDIT_BYTES {
        return Err("Value exceeds the 1 MiB edit limit");
    }
    match kind {
        Kind::Text => Ok(Cell::Text(text.to_owned())),
        Kind::Json => serde_json::from_str(text).map(Cell::Json).map_err(|_| "Invalid JSON"),
        Kind::JsonText | Kind::JsonFallbackText => {
            serde_json::from_str::<serde_json::Value>(text).map_err(|_| "Invalid JSON")?;
            Ok(if kind == Kind::JsonText {
                Cell::JsonText(text.to_owned())
            } else {
                Cell::Text(text.to_owned())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_text_and_json_text_and_refuses_invalid_or_oversized_values() {
        let text = "  hello\r\n世界\n";
        assert_eq!(payload(Kind::Text, text), Ok(Cell::Text(text.into())));
        assert_eq!(payload(Kind::Text, ""), Ok(Cell::Text(String::new())));
        assert_eq!(payload(Kind::Text, "\0tail"), Ok(Cell::Text("\0tail".into())));
        assert_eq!(payload(Kind::JsonText, " null "), Ok(Cell::JsonText(" null ".into())));
        assert_eq!(
            payload(Kind::JsonFallbackText, " {\"x\": 1} "),
            Ok(Cell::Text(" {\"x\": 1} ".into()))
        );
        assert_eq!(
            payload(Kind::Json, "null"),
            Ok(Cell::Json(Box::new(serde_json::Value::Null)))
        );
        assert!(payload(Kind::Json, "{").is_err());
        assert!(payload(Kind::JsonText, "").is_err());
        assert!(payload(Kind::JsonFallbackText, "not json").is_err());
        assert!(payload(Kind::Text, &"x".repeat(MAX_EDIT_BYTES)).is_ok());
        assert!(payload(Kind::Text, &"x".repeat(MAX_EDIT_BYTES + 1)).is_err());
    }

    #[test]
    fn structured_json_size_is_bounded_before_display() {
        assert!(json_fits(&serde_json::json!({"small": true})));
        assert!(!json_fits(&serde_json::Value::String("x".repeat(MAX_EDIT_BYTES))));
    }
}
