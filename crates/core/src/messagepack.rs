use serde_json::{Map, Number, Value as Json};

pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 10_000;
const EXACT_INTEGER: u64 = 1 << 53;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessagePackError {
    TooLarge,
    Truncated,
    TooDeep,
    TooManyValues,
    InvalidUtf8,
    Reserved,
    TrailingBytes,
    NotAContainer,
}

pub fn decode_messagepack(bytes: &[u8]) -> Result<Json, MessagePackError> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(MessagePackError::TooLarge);
    }
    let mut reader = Reader { bytes, at: 0, nodes: 0 };
    let value = reader.value(0)?;
    if reader.at != bytes.len() {
        return Err(MessagePackError::TrailingBytes);
    }
    if !matches!(value, Json::Array(_) | Json::Object(_)) {
        return Err(MessagePackError::NotAContainer);
    }
    Ok(value)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    nodes: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], MessagePackError> {
        let end = self.at.checked_add(count).ok_or(MessagePackError::Truncated)?;
        let slice = self.bytes.get(self.at..end).ok_or(MessagePackError::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    fn number<const N: usize>(&mut self) -> Result<[u8; N], MessagePackError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn length(&mut self, width: usize) -> Result<usize, MessagePackError> {
        let length = match width {
            1 => usize::from(u8::from_be_bytes(self.number()?)),
            2 => usize::from(u16::from_be_bytes(self.number()?)),
            _ => usize::try_from(u32::from_be_bytes(self.number()?)).map_err(|_| MessagePackError::TooLarge)?,
        };
        Ok(length)
    }

    fn string(&mut self, length: usize) -> Result<Json, MessagePackError> {
        let raw = self.take(length)?;
        std::str::from_utf8(raw)
            .map(|text| Json::String(text.to_string()))
            .map_err(|_| MessagePackError::InvalidUtf8)
    }

    fn binary(&mut self, length: usize) -> Result<Json, MessagePackError> {
        Ok(Json::String(hex(self.take(length)?)))
    }

    fn array(&mut self, length: usize, depth: usize) -> Result<Json, MessagePackError> {
        let mut items = Vec::new();
        for _ in 0..length {
            items.push(self.value(depth + 1)?);
        }
        Ok(Json::Array(items))
    }

    fn map(&mut self, length: usize, depth: usize) -> Result<Json, MessagePackError> {
        let mut entries = Map::new();
        for _ in 0..length {
            let key = match self.value(depth + 1)? {
                Json::String(text) => text,
                other => other.to_string(),
            };
            entries.insert(key, self.value(depth + 1)?);
        }
        Ok(Json::Object(entries))
    }

    fn extension(&mut self, length: usize) -> Result<Json, MessagePackError> {
        let kind = i8::from_be_bytes(self.number()?);
        let data = hex(self.take(length)?);
        Ok(serde_json::json!({ "ext": kind, "hex": data }))
    }

    fn value(&mut self, depth: usize) -> Result<Json, MessagePackError> {
        if depth > MAX_DEPTH {
            return Err(MessagePackError::TooDeep);
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(MessagePackError::TooManyValues);
        }
        let marker = u8::from_be_bytes(self.number()?);
        match marker {
            0x00..=0x7f => Ok(Json::from(marker)),
            0x80..=0x8f => self.map(usize::from(marker & 0x0f), depth),
            0x90..=0x9f => self.array(usize::from(marker & 0x0f), depth),
            0xa0..=0xbf => self.string(usize::from(marker & 0x1f)),
            0xc0 => Ok(Json::Null),
            0xc1 => Err(MessagePackError::Reserved),
            0xc2 => Ok(Json::Bool(false)),
            0xc3 => Ok(Json::Bool(true)),
            0xc4..=0xc9 | 0xd9..=0xdf => self.sized(marker, depth),
            0xca..=0xd3 => self.numeric(marker),
            0xd4..=0xd8 => self.extension(1 << (marker - 0xd4)),
            0xe0..=0xff => Ok(Json::from(i8::from_be_bytes([marker]))),
        }
    }

    fn numeric(&mut self, marker: u8) -> Result<Json, MessagePackError> {
        match marker {
            0xca => Ok(float(f64::from(f32::from_be_bytes(self.number()?)))),
            0xcb => Ok(float(f64::from_be_bytes(self.number()?))),
            0xcc => Ok(Json::from(u8::from_be_bytes(self.number()?))),
            0xcd => Ok(Json::from(u16::from_be_bytes(self.number()?))),
            0xce => Ok(Json::from(u32::from_be_bytes(self.number()?))),
            0xcf => Ok(unsigned(u64::from_be_bytes(self.number()?))),
            0xd0 => Ok(Json::from(i8::from_be_bytes(self.number()?))),
            0xd1 => Ok(Json::from(i16::from_be_bytes(self.number()?))),
            0xd2 => Ok(Json::from(i32::from_be_bytes(self.number()?))),
            _ => Ok(signed(i64::from_be_bytes(self.number()?))),
        }
    }

    fn sized(&mut self, marker: u8, depth: usize) -> Result<Json, MessagePackError> {
        let width = match marker {
            0xc4 | 0xc7 | 0xd9 => 1,
            0xc5 | 0xc8 | 0xda | 0xdc | 0xde => 2,
            _ => 4,
        };
        let length = self.length(width)?;
        match marker {
            0xc4..=0xc6 => self.binary(length),
            0xc7..=0xc9 => self.extension(length),
            0xd9..=0xdb => self.string(length),
            0xdc | 0xdd => self.array(length, depth),
            _ => self.map(length, depth),
        }
    }
}

fn float(number: f64) -> Json {
    Number::from_f64(number).map_or_else(|| Json::String(number.to_string()), Json::Number)
}

fn unsigned(number: u64) -> Json {
    if number > EXACT_INTEGER {
        Json::String(number.to_string())
    } else {
        Json::from(number)
    }
}

fn signed(number: i64) -> Json {
    if number.unsigned_abs() > EXACT_INTEGER {
        Json::String(number.to_string())
    } else {
        Json::from(number)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_map_with_text_numbers_nesting_and_nil_decodes() {
        let bytes = [
            0x84, 0xa1, b'a', 0x01, 0xa1, b'b', 0x92, 0xc3, 0xc0, 0xa1, b'c', 0xa2, b'h', b'i', 0xa1, b'd', 0xcb, 0x3f,
            0xf8, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(
            decode_messagepack(&bytes),
            Ok(json!({"a": 1, "b": [true, null], "c": "hi", "d": 1.5}))
        );
    }

    #[test]
    fn integers_of_every_width_and_sign_keep_their_value() {
        let bytes = [
            0x97, 0xff, 0xd0, 0x80, 0xcd, 0x01, 0x00, 0xd2, 0xff, 0xff, 0xff, 0xfe, 0xcf, 0x01, 0, 0, 0, 0, 0, 0, 1,
            0xd3, 0x80, 0, 0, 0, 0, 0, 0, 0, 0xce, 0xff, 0xff, 0xff, 0xff,
        ];
        assert_eq!(
            decode_messagepack(&bytes),
            Ok(json!([
                -1,
                -128,
                256,
                -2,
                "72057594037927937",
                "-9223372036854775808",
                4294967295u64
            ]))
        );
    }

    #[test]
    fn binary_and_extension_values_show_as_hex() {
        let bytes = [0x92, 0xc4, 0x02, 0xde, 0xad, 0xd4, 0x05, 0xff];
        assert_eq!(decode_messagepack(&bytes), Ok(json!(["dead", {"ext": 5, "hex": "ff"}])));
    }

    #[test]
    fn data_that_is_not_messagepack_stays_undecoded() {
        assert_eq!(
            decode_messagepack(&[0xde, 0xad, 0xbe, 0xef]),
            Err(MessagePackError::Truncated)
        );
        assert_eq!(decode_messagepack(&[0x92, 0x01]), Err(MessagePackError::Truncated));
        assert_eq!(decode_messagepack(&[0x91, 0xc1]), Err(MessagePackError::Reserved));
        assert_eq!(
            decode_messagepack(&[0x92, 0x01, 0x02, 0x03]),
            Err(MessagePackError::TrailingBytes)
        );
        assert_eq!(
            decode_messagepack(&[0xa2, b'h', b'i']),
            Err(MessagePackError::NotAContainer)
        );
        assert_eq!(
            decode_messagepack(&[0xa2, 0xff, 0xfe]),
            Err(MessagePackError::InvalidUtf8)
        );
        assert_eq!(
            decode_messagepack(&[0x91, 0xa2, 0xff, 0xfe]),
            Err(MessagePackError::InvalidUtf8)
        );
        assert_eq!(decode_messagepack(&[0x01]), Err(MessagePackError::NotAContainer));
        assert_eq!(decode_messagepack(&[]), Err(MessagePackError::Truncated));
    }

    #[test]
    fn hostile_depth_size_and_lengths_are_bounded() {
        let mut deep = vec![0x91; MAX_DEPTH + 5];
        deep.push(0xc0);
        assert_eq!(decode_messagepack(&deep), Err(MessagePackError::TooDeep));
        assert_eq!(
            decode_messagepack(&vec![0u8; MAX_PAYLOAD_BYTES + 1]),
            Err(MessagePackError::TooLarge)
        );
        assert_eq!(
            decode_messagepack(&[0xdd, 0xff, 0xff, 0xff, 0xff]),
            Err(MessagePackError::Truncated),
            "a declared length is not trusted to allocate"
        );
        let wide = {
            let mut bytes = vec![0xdc, 0x27, 0x11];
            bytes.extend(std::iter::repeat_n(0x01, 0x2711));
            bytes
        };
        assert_eq!(decode_messagepack(&wide), Err(MessagePackError::TooManyValues));
    }
}
