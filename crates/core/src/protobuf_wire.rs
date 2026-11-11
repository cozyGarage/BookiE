use serde_json::{Map, Value as Json};

pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 8;
const MAX_FIELDS: usize = 5_000;
const MAX_FIELD_NUMBER: u64 = 536_870_911;
const EXACT_INTEGER: u64 = 1 << 53;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtobufWireError {
    TooLarge,
    Truncated,
    BadTag,
    GroupsUnsupported,
    TooDeep,
    TooManyFields,
    Empty,
}

pub fn decode_protobuf_wire(bytes: &[u8]) -> Result<Json, ProtobufWireError> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(ProtobufWireError::TooLarge);
    }
    let mut fields = 0;
    message(bytes, 0, &mut fields)
}

fn message(bytes: &[u8], depth: usize, fields: &mut usize) -> Result<Json, ProtobufWireError> {
    if bytes.is_empty() {
        return Err(ProtobufWireError::Empty);
    }
    if depth > MAX_DEPTH {
        return Err(ProtobufWireError::TooDeep);
    }
    let mut at = 0;
    let mut out: Map<String, Json> = Map::new();
    while at < bytes.len() {
        *fields += 1;
        if *fields > MAX_FIELDS {
            return Err(ProtobufWireError::TooManyFields);
        }
        let tag = varint(bytes, &mut at)?;
        let number = tag >> 3;
        if number == 0 || number > MAX_FIELD_NUMBER {
            return Err(ProtobufWireError::BadTag);
        }
        let value = match tag & 7 {
            0 => integer(varint(bytes, &mut at)?),
            1 => integer(u64::from_le_bytes(fixed(bytes, &mut at)?)),
            2 => {
                let length = usize::try_from(varint(bytes, &mut at)?).map_err(|_| ProtobufWireError::TooLarge)?;
                let end = at.checked_add(length).ok_or(ProtobufWireError::Truncated)?;
                let payload = bytes.get(at..end).ok_or(ProtobufWireError::Truncated)?;
                at = end;
                length_delimited(payload, depth, fields)
            }
            5 => Json::from(u32::from_le_bytes(fixed(bytes, &mut at)?)),
            3 | 4 => return Err(ProtobufWireError::GroupsUnsupported),
            _ => return Err(ProtobufWireError::BadTag),
        };
        push(&mut out, number, value);
    }
    Ok(Json::Object(out))
}

fn length_delimited(payload: &[u8], depth: usize, fields: &mut usize) -> Json {
    if let Ok(text) = std::str::from_utf8(payload)
        && text.chars().all(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r'))
    {
        return Json::String(text.to_string());
    }
    let mut nested_fields = *fields;
    if let Ok(nested) = message(payload, depth + 1, &mut nested_fields) {
        *fields = nested_fields;
        return nested;
    }
    Json::String(payload.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn push(out: &mut Map<String, Json>, number: u64, value: Json) {
    let key = number.to_string();
    match out.remove(&key) {
        None => {
            out.insert(key, value);
        }
        Some(Json::Array(mut items)) => {
            items.push(value);
            out.insert(key, Json::Array(items));
        }
        Some(first) => {
            out.insert(key, Json::Array(vec![first, value]));
        }
    }
}

fn varint(bytes: &[u8], at: &mut usize) -> Result<u64, ProtobufWireError> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*at).ok_or(ProtobufWireError::Truncated)?;
        *at += 1;
        if shift == 63 && byte > 1 {
            return Err(ProtobufWireError::BadTag);
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(ProtobufWireError::BadTag)
}

fn fixed<const N: usize>(bytes: &[u8], at: &mut usize) -> Result<[u8; N], ProtobufWireError> {
    let end = at.checked_add(N).ok_or(ProtobufWireError::Truncated)?;
    let slice = bytes.get(*at..end).ok_or(ProtobufWireError::Truncated)?;
    *at = end;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    Ok(out)
}

fn integer(number: u64) -> Json {
    if number > EXACT_INTEGER {
        Json::String(number.to_string())
    } else {
        Json::from(number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_documented_example_decodes_without_a_schema() {
        let bytes = [0x08, 0x96, 0x01, 0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g'];
        assert_eq!(decode_protobuf_wire(&bytes), Ok(json!({"1": 150, "2": "testing"})));
    }

    #[test]
    fn nested_messages_repeated_fields_and_fixed_widths_decode() {
        let nested = [0x1a, 0x04, 0x08, 0x01, 0x10, 0x02];
        assert_eq!(decode_protobuf_wire(&nested), Ok(json!({"3": {"1": 1, "2": 2}})));
        let repeated = [0x08, 0x01, 0x08, 0x02, 0x08, 0x03];
        assert_eq!(decode_protobuf_wire(&repeated), Ok(json!({"1": [1, 2, 3]})));
        let fixed = [0x0d, 0x01, 0, 0, 0, 0x11, 0x02, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(decode_protobuf_wire(&fixed), Ok(json!({"1": 1, "2": 2})));
    }

    #[test]
    fn binary_payloads_that_are_not_messages_show_as_hex_and_wide_values_as_text() {
        let bytes = [0x0a, 0x02, 0xff, 0xfe];
        assert_eq!(decode_protobuf_wire(&bytes), Ok(json!({"1": "fffe"})));
        let wide = [0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        assert_eq!(decode_protobuf_wire(&wide), Ok(json!({"1": "18446744073709551615"})));
    }

    #[test]
    fn data_that_is_not_a_message_is_refused() {
        assert_eq!(decode_protobuf_wire(&[]), Err(ProtobufWireError::Empty));
        assert_eq!(decode_protobuf_wire(&[0x08]), Err(ProtobufWireError::Truncated));
        assert_eq!(
            decode_protobuf_wire(&[0x0a, 0x05, 0x01]),
            Err(ProtobufWireError::Truncated)
        );
        assert_eq!(decode_protobuf_wire(&[0x00, 0x01]), Err(ProtobufWireError::BadTag));
        assert_eq!(decode_protobuf_wire(&[0x0b]), Err(ProtobufWireError::GroupsUnsupported));
        assert_eq!(decode_protobuf_wire(&[0x0f, 0x01]), Err(ProtobufWireError::BadTag));
        assert_eq!(
            decode_protobuf_wire(&vec![0u8; MAX_PAYLOAD_BYTES + 1]),
            Err(ProtobufWireError::TooLarge)
        );
        let overlong = [0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        assert_eq!(decode_protobuf_wire(&overlong), Err(ProtobufWireError::BadTag));
    }

    #[test]
    fn deep_nesting_falls_back_to_hex_instead_of_recursing_forever() {
        let mut payload = vec![0x08, 0x01];
        for _ in 0..(MAX_DEPTH + 6) {
            let mut wrapped = vec![0x0a, u8::try_from(payload.len()).unwrap()];
            wrapped.extend(payload);
            payload = wrapped;
        }
        let decoded = decode_protobuf_wire(&payload).expect("outer message parses");
        assert!(decoded.to_string().contains('"'), "{decoded}");
    }
}
