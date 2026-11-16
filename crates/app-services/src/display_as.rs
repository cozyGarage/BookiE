use bookie_core::Value;
use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

pub const MAX_JSON_TEXT_BYTES: usize = 256 * 1024;
const MIN_UNIX_SECONDS: i64 = 946_684_800;
const MAX_UNIX_SECONDS: i64 = 4_102_444_800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayKind {
    UnixSeconds,
    UnixMilliseconds,
    UnixMicroseconds,
    UuidHyphenated,
    UuidUppercase,
    UuidSimple,
    UuidBraced,
    UuidUrn,
    JsonInText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayForm {
    pub kind: DisplayKind,
    pub text: String,
}

pub fn display_forms(value: &Value) -> Vec<DisplayForm> {
    match value {
        Value::Int(number) => unix_time_form(*number).into_iter().collect(),
        Value::Uuid(uuid) => uuid_forms(uuid, None),
        Value::Text(text) => text_forms(text),
        Value::Bytes(bytes) => <[u8; 16]>::try_from(bytes.as_slice())
            .map(|raw| uuid_forms(&Uuid::from_bytes(raw), None))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn unix_time_form(number: i64) -> Option<DisplayForm> {
    let (kind, divisor) = match number {
        MIN_UNIX_SECONDS..=MAX_UNIX_SECONDS => (DisplayKind::UnixSeconds, 1),
        _ if (MIN_UNIX_SECONDS * 1_000..=MAX_UNIX_SECONDS * 1_000).contains(&number) => {
            (DisplayKind::UnixMilliseconds, 1_000)
        }
        _ if (MIN_UNIX_SECONDS * 1_000_000..=MAX_UNIX_SECONDS * 1_000_000).contains(&number) => {
            (DisplayKind::UnixMicroseconds, 1_000_000)
        }
        _ => return None,
    };
    let seconds = number.div_euclid(divisor);
    let nanos = u32::try_from(number.rem_euclid(divisor) * (1_000_000_000 / divisor)).ok()?;
    let moment = DateTime::<Utc>::from_timestamp(seconds, nanos)?;
    Some(DisplayForm {
        kind,
        text: moment.to_rfc3339_opts(SecondsFormat::AutoSi, true),
    })
}

fn text_forms(text: &str) -> Vec<DisplayForm> {
    if text.len() > MAX_JSON_TEXT_BYTES {
        return Vec::new();
    }
    if let Ok(uuid) = Uuid::parse_str(text.trim()) {
        return uuid_forms(&uuid, Some(text));
    }
    json_form(text).into_iter().collect()
}

fn uuid_forms(uuid: &Uuid, original: Option<&str>) -> Vec<DisplayForm> {
    let candidates = [
        (DisplayKind::UuidHyphenated, uuid.hyphenated().to_string()),
        (DisplayKind::UuidUppercase, uuid.hyphenated().to_string().to_uppercase()),
        (DisplayKind::UuidSimple, uuid.simple().to_string()),
        (DisplayKind::UuidBraced, uuid.braced().to_string()),
        (DisplayKind::UuidUrn, uuid.urn().to_string()),
    ];
    candidates
        .into_iter()
        .filter(|(_, text)| original != Some(text.as_str()))
        .map(|(kind, text)| DisplayForm { kind, text })
        .collect()
}

fn json_form(text: &str) -> Option<DisplayForm> {
    let trimmed = text.trim();
    if !matches!(trimmed.as_bytes().first(), Some(b'{' | b'[')) {
        return None;
    }
    let pretty = crate::json_format::pretty_json(text, MAX_JSON_TEXT_BYTES).ok()?;
    if pretty == text {
        return None;
    }
    Some(DisplayForm {
        kind: DisplayKind::JsonInText,
        text: pretty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(value: &Value) -> Vec<DisplayKind> {
        display_forms(value).into_iter().map(|form| form.kind).collect()
    }

    fn only(value: &Value) -> DisplayForm {
        let mut forms = display_forms(value);
        assert_eq!(forms.len(), 1, "{forms:?}");
        forms.remove(0)
    }

    #[test]
    fn integers_in_the_plausible_range_read_as_utc_time_in_the_right_unit() {
        let seconds = only(&Value::Int(1_700_000_000));
        assert_eq!(seconds.kind, DisplayKind::UnixSeconds);
        assert_eq!(seconds.text, "2023-11-14T22:13:20Z");
        let millis = only(&Value::Int(1_700_000_000_123));
        assert_eq!(millis.kind, DisplayKind::UnixMilliseconds);
        assert_eq!(millis.text, "2023-11-14T22:13:20.123Z");
        let micros = only(&Value::Int(1_700_000_000_123_456));
        assert_eq!(micros.kind, DisplayKind::UnixMicroseconds);
        assert_eq!(micros.text, "2023-11-14T22:13:20.123456Z");
    }

    #[test]
    fn integers_outside_the_plausible_range_get_no_time_form() {
        for number in [0, 5, 42, -1, 946_684_799, 4_102_444_801, i64::MIN, i64::MAX] {
            assert!(display_forms(&Value::Int(number)).is_empty(), "{number}");
        }
    }

    #[test]
    fn sixteen_bytes_read_as_a_uuid_and_other_lengths_do_not() {
        let raw: [u8; 16] = *b"\x12\x3e\x45\x67\xe8\x9b\x12\xd3\xa4\x56\x42\x66\x14\x17\x40\x00";
        let forms = display_forms(&Value::Bytes(raw.to_vec()));
        assert_eq!(forms[0].kind, DisplayKind::UuidHyphenated);
        assert_eq!(forms[0].text, "123e4567-e89b-12d3-a456-426614174000");
        assert!(display_forms(&Value::Bytes(vec![1; 15])).is_empty());
        assert!(display_forms(&Value::Bytes(vec![1; 17])).is_empty());
    }

    #[test]
    fn uuid_text_offers_the_other_spellings_but_not_its_own() {
        let value = Value::Text("123e4567-e89b-12d3-a456-426614174000".into());
        assert_eq!(
            kinds(&value),
            [
                DisplayKind::UuidUppercase,
                DisplayKind::UuidSimple,
                DisplayKind::UuidBraced,
                DisplayKind::UuidUrn
            ]
        );
        let simple = display_forms(&Value::Text("123E4567E89B12D3A456426614174000".into()));
        assert!(simple.iter().any(|form| form.kind == DisplayKind::UuidHyphenated));
        assert!(
            simple
                .iter()
                .all(|form| form.text != "123E4567E89B12D3A456426614174000")
        );
    }

    #[test]
    fn typed_uuid_offers_every_spelling() {
        let uuid = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").expect("uuid");
        assert_eq!(display_forms(&Value::Uuid(uuid)).len(), 5);
    }

    #[test]
    fn json_inside_text_is_pretty_printed_once() {
        let form = only(&Value::Text("{\"a\":1,\"b\":[true]}".into()));
        assert_eq!(form.kind, DisplayKind::JsonInText);
        assert!(form.text.contains("\"a\": 1"));
        assert!(display_forms(&Value::Text(form.text)).is_empty());
    }

    #[test]
    fn json_display_preserves_duplicate_entries_and_number_spellings() {
        let source = r#"{"a":1,"a":2,"n":900719925474099312345.000,"e":1e+004,"s":"\u0041"}"#;
        let form = json_form(source).unwrap();
        assert_eq!(
            form.text,
            r#"{
  "a": 1,
  "a": 2,
  "n": 900719925474099312345.000,
  "e": 1e+004,
  "s": "\u0041"
}"#
        );
    }

    #[test]
    fn json_display_refuses_oversized_expansion_and_whitespace() {
        let nested = format!("{}[{}0]{}", "[".repeat(100), "0,".repeat(10_000), "]".repeat(100));
        assert!(nested.len() < MAX_JSON_TEXT_BYTES);
        assert!(json_form(&nested).is_none());
        let padded = format!("{}{{\"a\":1}}", " ".repeat(MAX_JSON_TEXT_BYTES));
        assert!(json_form(&padded).is_none());
    }

    #[test]
    fn plain_text_and_scalars_in_text_get_no_form() {
        for text in ["hello", "12", "true", "{not json", "[1,", ""] {
            assert!(display_forms(&Value::Text(text.into())).is_empty(), "{text}");
        }
    }

    #[test]
    fn oversized_json_text_is_not_parsed() {
        let big = format!("[{}0]", "0,".repeat(MAX_JSON_TEXT_BYTES));
        assert!(display_forms(&Value::Text(big)).is_empty());
    }

    #[test]
    fn other_value_kinds_get_no_form() {
        for value in [
            Value::Null,
            Value::Bool(true),
            Value::Float(1.7e9),
            Value::JsonText("{\"a\":1}".into()),
        ] {
            assert!(display_forms(&value).is_empty());
        }
    }
}
