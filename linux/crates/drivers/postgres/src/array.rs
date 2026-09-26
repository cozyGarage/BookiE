use sqlx::ValueRef;
use sqlx::postgres::{PgTypeKind, PgValueFormat, PgValueRef};
use tablepro_core::Value;

const MAX_ARRAY_TEXT_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn decode(raw: &PgValueRef<'_>) -> Option<Value> {
    let info = raw.type_info();
    let PgTypeKind::Array(element) = info.kind() else {
        return None;
    };
    match raw.format() {
        PgValueFormat::Binary => decode_binary(raw.as_bytes().ok()?, element.oid()?.0).map(Value::Text),
        PgValueFormat::Text => raw.as_str().ok().map(|text| Value::Text(text.into())),
    }
}

struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (value, remaining) = self.remaining.split_at_checked(count)?;
        self.remaining = remaining;
        Some(value)
    }

    fn integer(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
}

struct Dimension {
    length: usize,
    lower: i32,
    upper: i32,
}

fn read_dimensions(reader: &mut Reader<'_>, count: usize) -> Option<(Vec<Dimension>, usize)> {
    let mut dimensions = Vec::with_capacity(count);
    let mut total = if count == 0 { 0_usize } else { 1 };
    for _ in 0..count {
        let length = reader.integer()?;
        let lower = reader.integer()?;
        if length <= 0 {
            return None;
        }
        let upper = lower.checked_add(length - 1)?;
        let length = usize::try_from(length).ok()?;
        total = total.checked_mul(length)?;
        dimensions.push(Dimension { length, lower, upper });
    }
    (total <= reader.remaining.len() / 4).then_some((dimensions, total))
}

fn decode_binary(bytes: &[u8], expected_oid: u32) -> Option<String> {
    let mut reader = Reader { remaining: bytes };
    let count = reader.integer()?;
    let flags = reader.integer()?;
    let oid = reader.integer()? as u32;
    if !(0..=6).contains(&count) || !matches!(flags, 0 | 1) || oid != expected_oid || !supported(oid) {
        return None;
    }
    let (dimensions, total) = read_dimensions(&mut reader, count as usize)?;
    let mut output = String::new();
    if dimensions.iter().any(|dimension| dimension.lower != 1) {
        for dimension in &dimensions {
            output.push_str(&format!("[{}:{}]", dimension.lower, dimension.upper));
        }
        output.push('=');
    }
    if total == 0 {
        output.push_str("{}");
    } else {
        append_dimension(&mut reader, &dimensions, oid, flags == 1, &mut output)?;
    }
    reader.remaining.is_empty().then_some(output)
}

fn append_dimension(
    reader: &mut Reader<'_>,
    dimensions: &[Dimension],
    oid: u32,
    allows_null: bool,
    output: &mut String,
) -> Option<()> {
    let (dimension, rest) = dimensions.split_first()?;
    output.push('{');
    for index in 0..dimension.length {
        if index > 0 {
            output.push(',');
        }
        if rest.is_empty() {
            let length = reader.integer()?;
            if length == -1 && allows_null {
                output.push_str("NULL");
            } else {
                let length = usize::try_from(length).ok()?;
                if length > MAX_ARRAY_TEXT_BYTES {
                    return None;
                }
                let bytes = reader.take(length)?;
                append_quoted(output, &element_text(oid, bytes)?)?;
            }
        } else {
            append_dimension(reader, rest, oid, allows_null, output)?;
        }
        if output.len() >= MAX_ARRAY_TEXT_BYTES {
            return None;
        }
    }
    output.push('}');
    Some(())
}

fn append_quoted(output: &mut String, text: &str) -> Option<()> {
    let escapes = text.bytes().filter(|byte| matches!(byte, b'"' | b'\\')).count();
    let length = output
        .len()
        .checked_add(text.len())?
        .checked_add(escapes)?
        .checked_add(2)?;
    if length > MAX_ARRAY_TEXT_BYTES {
        return None;
    }
    output.push('"');
    for character in text.chars() {
        if matches!(character, '"' | '\\') {
            output.push('\\');
        }
        output.push(character);
    }
    output.push('"');
    Some(())
}

fn supported(oid: u32) -> bool {
    matches!(
        oid,
        16 | 17 | 19 | 20 | 21 | 23 | 25 | 26 | 700 | 701 | 1042 | 1043 | 1700 | 2950
    )
}

fn element_text(oid: u32, bytes: &[u8]) -> Option<String> {
    Some(match oid {
        16 => match bytes {
            [0] => "f".into(),
            [1] => "t".into(),
            _ => return None,
        },
        17 => format!(
            "\\x{}",
            bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        ),
        19 | 25 | 1042 | 1043 if !bytes.contains(&0) => std::str::from_utf8(bytes).ok()?.into(),
        20 => i64::from_be_bytes(bytes.try_into().ok()?).to_string(),
        21 => i16::from_be_bytes(bytes.try_into().ok()?).to_string(),
        23 => i32::from_be_bytes(bytes.try_into().ok()?).to_string(),
        26 => u32::from_be_bytes(bytes.try_into().ok()?).to_string(),
        700 => {
            let value = f32::from_be_bytes(bytes.try_into().ok()?);
            if value.is_finite() {
                value.to_string()
            } else {
                float_text(f64::from(value))
            }
        }
        701 => float_text(f64::from_be_bytes(bytes.try_into().ok()?)),
        1700 => match crate::numeric::decode_binary(bytes)? {
            Value::Decimal(value) => value.to_string(),
            Value::Text(value) => value,
            _ => return None,
        },
        2950 => uuid::Uuid::from_slice(bytes).ok()?.to_string(),
        _ => return None,
    })
}

fn float_text(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(oid: u32, dimensions: &[(i32, i32)], elements: &[Option<&[u8]>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for word in [dimensions.len() as i32, i32::from(elements.contains(&None)), oid as i32] {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
        for (length, lower) in dimensions {
            bytes.extend_from_slice(&length.to_be_bytes());
            bytes.extend_from_slice(&lower.to_be_bytes());
        }
        for element in elements {
            bytes.extend_from_slice(&element.map_or(-1, |value| value.len() as i32).to_be_bytes());
            if let Some(value) = element {
                bytes.extend_from_slice(value);
            }
        }
        bytes
    }

    #[test]
    fn value_contract_array_null_empty_and_escaped_text_stay_distinct() {
        assert_eq!(decode_binary(&wire(25, &[], &[]), 25).as_deref(), Some("{}"));
        let bytes = wire(
            25,
            &[(5, 1)],
            &[None, Some(b"NULL"), Some(b""), Some(b"a\\\"b"), Some("漢字".as_bytes())],
        );
        assert_eq!(
            decode_binary(&bytes, 25).as_deref(),
            Some(r#"{NULL,"NULL","","a\\\"b","漢字"}"#)
        );
        let integer = 1_i32.to_be_bytes();
        let bytes = wire(
            23,
            &[(2, 0), (2, -1)],
            &[Some(&integer), None, Some(&integer), Some(&integer)],
        );
        assert_eq!(
            decode_binary(&bytes, 23).as_deref(),
            Some(r#"[0:1][-1:0]={{"1",NULL},{"1","1"}}"#)
        );
    }

    #[test]
    fn malformed_array_dimensions_lengths_types_and_elements_are_refused() {
        let valid = wire(25, &[(1, 1)], &[Some(b"value")]);
        for length in 0..valid.len() {
            assert_eq!(decode_binary(&valid[..length], 25), None);
        }
        assert_eq!(decode_binary(&valid, 23), None);
        for (offset, word) in [
            (0, -1_i32),
            (0, 7),
            (4, 2),
            (12, 0),
            (12, i32::MAX),
            (20, -2),
            (20, i32::MAX),
        ] {
            let mut bytes = valid.clone();
            bytes[offset..offset + 4].copy_from_slice(&word.to_be_bytes());
            assert_eq!(decode_binary(&bytes, 25), None, "{offset}: {word}");
        }
        let mut trailing = valid;
        trailing.push(0);
        assert_eq!(decode_binary(&trailing, 25), None);
        let mut forbidden_null = wire(25, &[(1, 1)], &[None]);
        forbidden_null[4..8].copy_from_slice(&0_i32.to_be_bytes());
        assert_eq!(decode_binary(&forbidden_null, 25), None);
        for bytes in [
            wire(25, &[(2, i32::MAX)], &[None, None]),
            wire(25, &[(i32::MAX, 1); 6], &[]),
            wire(25, &[(1, 1)], &[Some(&[255])]),
            wire(25, &[(1, 1)], &[Some(b"a\0b")]),
        ] {
            assert_eq!(decode_binary(&bytes, 25), None);
        }
        assert_eq!(decode_binary(&wire(9999, &[], &[]), 9999), None);
        assert_eq!(element_text(16, &[2]), None);
        assert_eq!(element_text(23, &[1, 2]), None);
    }

    #[test]
    fn array_header_mutations_and_maximum_depth_are_bounded() {
        let bytes = wire(25, &[(1, i32::MIN); 6], &[Some(b"value")]);
        assert!(decode_binary(&bytes, 25).is_some());
        for offset in 0..bytes.len() {
            for replacement in [0, 1, 127, 128, 254, 255] {
                let mut mutated = bytes.clone();
                mutated[offset] = replacement;
                let _ = decode_binary(&mutated, 25);
            }
        }
        let mut state = 12345_u32;
        for length in 0..128 {
            let mut bytes = vec![0; length];
            for byte in &mut bytes {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                *byte = (state >> 24) as u8;
            }
            let _ = decode_binary(&bytes, 25);
        }
    }

    #[test]
    fn expanded_array_text_has_an_explicit_size_bound() {
        let mut output = String::new();
        assert!(append_quoted(&mut output, &"x".repeat(MAX_ARRAY_TEXT_BYTES)).is_none());
        assert!(output.is_empty());
        assert!(append_quoted(&mut output, &"\\".repeat(MAX_ARRAY_TEXT_BYTES / 2)).is_none());
        assert!(output.is_empty());
    }
}
