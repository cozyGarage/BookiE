pub const MAX_PREVIEW_BYTES: usize = 256 * 1024;
const MIN_PROTOBUF_FIELDS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodedFormat {
    MessagePack,
    Protobuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPreview {
    pub format: DecodedFormat,
    pub text: String,
    pub truncated: bool,
}

pub fn decoded_preview(bytes: &[u8]) -> Option<DecodedPreview> {
    if let Ok(value) = bookie_core::decode_messagepack(bytes) {
        return preview(DecodedFormat::MessagePack, &value);
    }
    let printable = bytes
        .iter()
        .all(|byte| byte.is_ascii_graphic() || byte.is_ascii_whitespace());
    if printable {
        return None;
    }
    let value = bookie_core::decode_protobuf_wire(bytes).ok()?;
    let fields = value.as_object().map_or(0, serde_json::Map::len);
    if fields < MIN_PROTOBUF_FIELDS {
        return None;
    }
    preview(DecodedFormat::Protobuf, &value)
}

fn preview(format: DecodedFormat, value: &serde_json::Value) -> Option<DecodedPreview> {
    let pretty = serde_json::to_string_pretty(value).ok()?;
    let (text, truncated) = bounded(pretty);
    Some(DecodedPreview {
        format,
        text,
        truncated,
    })
}

fn bounded(mut text: String) -> (String, bool) {
    if text.len() <= MAX_PREVIEW_BYTES {
        return (text, false);
    }
    let mut end = MAX_PREVIEW_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_map() -> Vec<u8> {
        vec![0x82, 0xa1, b'a', 0x01, 0xa1, b'b', 0xc3]
    }

    fn protobuf_message() -> Vec<u8> {
        vec![0x08, 0x96, 0x01, 0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g']
    }

    #[test]
    fn messagepack_maps_decode_to_pretty_json() {
        let preview = decoded_preview(&pack_map()).expect("a map decodes");
        assert_eq!(preview.format, DecodedFormat::MessagePack);
        assert!(preview.text.contains("\"a\": 1") && preview.text.contains("true"));
        assert!(!preview.truncated);
    }

    #[test]
    fn protobuf_messages_decode_by_field_number() {
        let preview = decoded_preview(&protobuf_message()).expect("two fields decode");
        assert_eq!(preview.format, DecodedFormat::Protobuf);
        assert!(preview.text.contains("\"1\": 150") && preview.text.contains("\"2\": \"testing\""));
    }

    #[test]
    fn printable_text_is_not_taken_for_protobuf() {
        assert_eq!(decoded_preview(b"\"hello world\" is text"), None);
    }

    #[test]
    fn a_single_protobuf_field_is_too_weak_a_signal() {
        assert_eq!(decoded_preview(&[0x08, 0x96, 0x01]), None);
    }

    #[test]
    fn unrecognised_bytes_have_no_preview() {
        assert_eq!(decoded_preview(&[0xde, 0xad, 0xbe, 0xef]), None);
        assert_eq!(decoded_preview(&[]), None);
    }

    #[test]
    fn oversized_input_is_not_decoded() {
        let mut bytes = vec![0xdc, 0xff, 0xff];
        bytes.resize(bookie_core::MAX_MESSAGEPACK_BYTES + 1, 0xc0);
        assert_eq!(decoded_preview(&bytes), None);
    }

    #[test]
    fn hostile_nesting_and_truncation_fail_without_a_preview() {
        assert_eq!(decoded_preview(&[0x91; 200]), None);
        assert_eq!(decoded_preview(&[0x92, 0x01]), None);
        assert_eq!(decoded_preview(&[0x0a, 0x09, 0x01, 0x10, 0x01]), None);
    }

    #[test]
    fn a_large_decoded_text_is_cut_on_a_character_boundary() {
        let (text, truncated) = bounded("é".repeat(MAX_PREVIEW_BYTES));
        assert!(truncated);
        assert!(text.len() <= MAX_PREVIEW_BYTES);
        assert!(text.chars().all(|c| c == 'é'));
        assert_eq!(bounded("small".into()), ("small".into(), false));
    }
}
