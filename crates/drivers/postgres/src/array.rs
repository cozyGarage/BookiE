use sqlx::postgres::{PgTypeInfo, PgTypeKind, PgValueFormat, PgValueRef};
use sqlx::{TypeInfo, ValueRef};
use tablepro_core::Value;

pub(super) const MAX_ARRAY_TEXT_BYTES: usize = 16 * 1024 * 1024;
// PostgreSQL sends int2vector and oidvector in array wire format, but their text input accepts only
// space-separated values, so array text for them could not be imported again.
const INT2VECTOR_OID: u32 = 22;
const OIDVECTOR_OID: u32 = 30;
const BIT_OID: u32 = 1560;
const VARBIT_OID: u32 = 1562;
const MACADDR8_OID: u32 = 774;
const MACADDR_OID: u32 = 829;
const INET_OID: u32 = 869;
const CIDR_OID: u32 = 650;
const PG_LSN_OID: u32 = 3220;
const XML_OID: u32 = 142;

pub(crate) fn decode(raw: &PgValueRef<'_>) -> Option<Value> {
    let info = raw.type_info();
    let PgTypeKind::Array(element) = info.kind() else {
        return None;
    };
    let element_oid = element.oid()?.0;
    let value_oid = base_oid(element)?;
    let text_element = is_text_element(element);
    match raw.format() {
        PgValueFormat::Binary => {
            let bytes = raw.as_bytes().ok()?;
            let text = match info.oid()?.0 {
                INT2VECTOR_OID | OIDVECTOR_OID => decode_vector(bytes, element_oid),
                _ => decode_binary_with_oid(bytes, element_oid, value_oid, text_element),
            }?;
            Some(Value::Text(text))
        }
        PgValueFormat::Text => raw.as_str().ok().map(|text| Value::Text(text.into())),
    }
}

pub(super) fn is_text_element(info: &PgTypeInfo) -> bool {
    let mut current = info;
    loop {
        match current.kind() {
            PgTypeKind::Enum(_) => return true,
            PgTypeKind::Domain(base) => current = base,
            // ponytail: PgTypeInfo omits custom namespaces; if unrelated
            // simple types named citext are used, verify extension membership by OID.
            PgTypeKind::Simple => {
                return current.name().eq_ignore_ascii_case("citext")
                    || current.oid().is_some_and(|oid| oid.0 == XML_OID);
            }
            _ => return false,
        }
    }
}

fn base_oid(info: &PgTypeInfo) -> Option<u32> {
    match info.kind() {
        PgTypeKind::Domain(base) => base_oid(base),
        _ => info.oid().map(|oid| oid.0),
    }
}

fn decode_vector(bytes: &[u8], expected_oid: u32) -> Option<String> {
    let mut reader = Reader { remaining: bytes };
    let count = reader.integer()?;
    let flags = reader.integer()?;
    let oid = reader.integer()? as u32;
    if count != 1 || flags != 0 || oid != expected_oid {
        return None;
    }
    let length = usize::try_from(reader.integer()?).ok()?;
    if reader.integer()? != 0 {
        return None;
    }
    let mut elements = Vec::with_capacity(vector_capacity(length, reader.remaining.len())?);
    for _ in 0..length {
        let length = reader.integer()?;
        elements.push(element_text(oid, false, read_bounded_element(&mut reader, length)?)?);
    }
    reader.remaining.is_empty().then(|| elements.join(" "))
}

fn vector_capacity(length: usize, payload_bytes: usize) -> Option<usize> {
    (length <= payload_bytes / 4).then_some(length)
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

#[cfg(test)]
fn decode_binary(bytes: &[u8], expected_oid: u32, text_element: bool) -> Option<String> {
    decode_binary_with_oid(bytes, expected_oid, expected_oid, text_element)
}

fn decode_binary_with_oid(bytes: &[u8], expected_oid: u32, value_oid: u32, text_element: bool) -> Option<String> {
    let mut reader = Reader { remaining: bytes };
    let count = reader.integer()?;
    let flags = reader.integer()?;
    let oid = reader.integer()? as u32;
    if !(0..=6).contains(&count)
        || !matches!(flags, 0 | 1)
        || oid != expected_oid
        || (!text_element && !supported(value_oid))
    {
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
        append_dimension(
            &mut reader,
            &dimensions,
            value_oid,
            text_element,
            flags == 1,
            &mut output,
        )?;
    }
    reader.remaining.is_empty().then_some(output)
}

fn append_dimension(
    reader: &mut Reader<'_>,
    dimensions: &[Dimension],
    oid: u32,
    text_element: bool,
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
                let bytes = read_bounded_element(reader, length)?;
                append_quoted(output, &element_text(oid, text_element, bytes)?)?;
            }
        } else {
            append_dimension(reader, rest, oid, text_element, allows_null, output)?;
        }
        if output.len() >= MAX_ARRAY_TEXT_BYTES {
            return None;
        }
    }
    output.push('}');
    Some(())
}

fn read_bounded_element<'a>(reader: &mut Reader<'a>, length: i32) -> Option<&'a [u8]> {
    let length = usize::try_from(length).ok()?;
    if length > MAX_ARRAY_TEXT_BYTES {
        return None;
    }
    reader.take(length)
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
        16 | 17
            | 19
            | 20
            | 21
            | 23
            | 25
            | 26
            | 700
            | 701
            | 1042
            | 1043
            | 1082
            | 1083
            | 1114
            | 1184
            | 1186
            | 1266
            | XML_OID
            | BIT_OID
            | VARBIT_OID
            | 114
            | 3802
            | 1700
            | 2950
            | MACADDR_OID
            | MACADDR8_OID
            | INET_OID
            | CIDR_OID
            | PG_LSN_OID
    )
}

fn element_text(oid: u32, text_element: bool, bytes: &[u8]) -> Option<String> {
    if text_element {
        return Some(std::str::from_utf8(bytes).ok()?.into());
    }
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
        BIT_OID | VARBIT_OID => {
            crate::decode::decode_pg_binary_text(if oid == BIT_OID { "BIT" } else { "VARBIT" }, bytes)?
        }
        MACADDR_OID => crate::decode::decode_pg_binary_text("MACADDR", bytes)?,
        MACADDR8_OID => crate::decode::decode_pg_binary_text("MACADDR8", bytes)?,
        INET_OID => crate::decode::decode_pg_binary_text("INET", bytes)?,
        CIDR_OID => crate::decode::decode_pg_binary_text("CIDR", bytes)?,
        20 => i64::from_be_bytes(bytes.try_into().ok()?).to_string(),
        21 => i16::from_be_bytes(bytes.try_into().ok()?).to_string(),
        23 => i32::from_be_bytes(bytes.try_into().ok()?).to_string(),
        26 => u32::from_be_bytes(bytes.try_into().ok()?).to_string(),
        700 => crate::decode::float4_text(f32::from_be_bytes(bytes.try_into().ok()?)),
        701 => crate::decode::float8_text(f64::from_be_bytes(bytes.try_into().ok()?)),
        1700 => match crate::numeric::decode_binary(bytes)? {
            Value::Decimal(value) => value.to_string(),
            Value::Text(value) => value,
            _ => return None,
        },
        2950 => uuid::Uuid::from_slice(bytes).ok()?.to_string(),
        114 => std::str::from_utf8(bytes).ok()?.to_string(),
        3802 => std::str::from_utf8(bytes.strip_prefix(&[1])?).ok()?.to_string(),
        PG_LSN_OID => crate::decode::decode_pg_binary_text("PG_LSN", bytes)?,
        1082 | 1083 | 1114 | 1184 | 1186 | 1266 => temporal_element(oid, bytes)?,
        _ => return None,
    })
}

fn temporal_element(oid: u32, bytes: &[u8]) -> Option<String> {
    let value = match oid {
        1082 => crate::temporal::decode_temporal(bytes, "DATE")?,
        1083 | 1266 => crate::temporal::decode_time(bytes, oid == 1266)?,
        1114 => crate::temporal::decode_temporal(bytes, "TIMESTAMP")?,
        1184 => crate::temporal::decode_temporal(bytes, "TIMESTAMPTZ")?,
        1186 => return crate::decode::decode_pg_binary_text("INTERVAL", bytes),
        _ => return None,
    };
    crate::temporal::array_text(value)
}

#[cfg(test)]
#[path = "array_tests.rs"]
mod tests;
