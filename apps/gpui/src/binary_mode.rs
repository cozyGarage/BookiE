use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BinaryMode {
    #[default]
    Hex,
    Base64,
    Utf8,
    ByteCount,
}

impl BinaryMode {
    pub const ALL: [BinaryMode; 4] = [
        BinaryMode::Hex,
        BinaryMode::Base64,
        BinaryMode::Utf8,
        BinaryMode::ByteCount,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BinaryMode::Hex => "Hex",
            BinaryMode::Base64 => "Base64",
            BinaryMode::Utf8 => "UTF-8",
            BinaryMode::ByteCount => "Byte count",
        }
    }

    pub fn format(self, bytes: &[u8]) -> String {
        match self {
            BinaryMode::Hex => format!("\\x{}", hex_encode(bytes)),
            BinaryMode::Base64 => base64_encode(bytes),
            BinaryMode::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            BinaryMode::ByteCount => format!("{} bytes", bytes.len()),
        }
    }
}

pub fn is_binary_type(type_name: &str) -> bool {
    let lower = type_name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "bytea" | "bytes" | "blob" | "tinyblob" | "mediumblob" | "longblob" | "binary" | "varbinary" | "raw"
    )
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        let n = (b0 << 16) | (b1 << 8) | b2;
        let _ = write!(out, "{}", ALPHABET[n >> 18] as char);
        let _ = write!(out, "{}", ALPHABET[(n >> 12) & 0x3f] as char);
        if chunk.len() > 1 {
            let _ = write!(out, "{}", ALPHABET[(n >> 6) & 0x3f] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            let _ = write!(out, "{}", ALPHABET[n & 0x3f] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_formats_bytes() {
        assert_eq!(BinaryMode::Hex.format(&[0xDE, 0xAD]), "\\xdead");
    }

    #[test]
    fn base64_formats_bytes() {
        assert_eq!(BinaryMode::Base64.format(b"Man"), "TWFu");
    }

    #[test]
    fn byte_count_formats_bytes() {
        assert_eq!(BinaryMode::ByteCount.format(&[1, 2, 3]), "3 bytes");
    }

    #[test]
    fn utf8_formats_bytes() {
        assert_eq!(BinaryMode::Utf8.format(b"hello"), "hello");
    }

    #[test]
    fn binary_type_matching() {
        assert!(is_binary_type("bytea"));
        assert!(is_binary_type("BLOB"));
        assert!(is_binary_type("varbinary"));
        assert!(!is_binary_type("integer"));
        assert!(!is_binary_type("text"));
    }
}
