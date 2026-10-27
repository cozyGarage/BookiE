pub(crate) fn decode_pg_binary_text(type_name: &str, bytes: &[u8]) -> Option<String> {
    match type_name {
        "INTERVAL" => decode_interval(bytes),
        "BIT" | "VARBIT" => decode_bit_string(bytes),
        "MACADDR" => decode_macaddr(bytes),
        "MACADDR8" => decode_macaddr8(bytes),
        "INET" => decode_inet(bytes, false),
        "CIDR" => decode_inet(bytes, true),
        "PG_LSN" => decode_pg_lsn(bytes),
        "XML" => String::from_utf8(bytes.to_vec()).ok(),
        "TSVECTOR" => decode_tsvector(bytes),
        "POINT" => decode_point(bytes),
        "INT4RANGE" => decode_int_range(bytes, 4),
        "INT8RANGE" => decode_int_range(bytes, 8),
        "OID" | "XID" | "CID" => Some(u32::from_be_bytes(bytes.try_into().ok()?).to_string()),
        _ => None,
    }
}

fn decode_macaddr(bytes: &[u8]) -> Option<String> {
    let [a, b, c, d, e, f] = bytes else {
        return None;
    };
    Some(format!("{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{f:02x}"))
}

fn decode_macaddr8(bytes: &[u8]) -> Option<String> {
    let [a, b, c, d, e, f, g, h] = bytes else {
        return None;
    };
    Some(format!(
        "{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{f:02x}:{g:02x}:{h:02x}"
    ))
}

fn decode_bit_string(bytes: &[u8]) -> Option<String> {
    let bit_length = usize::try_from(i32::from_be_bytes(bytes.get(..4)?.try_into().ok()?)).ok()?;
    let byte_length = bit_length.checked_add(7)? / 8;
    if bytes.len() != 4 + byte_length {
        return None;
    }
    if bit_length % 8 != 0 {
        let unused = 8 - bit_length % 8;
        if bytes[4 + byte_length - 1] & ((1_u8 << unused) - 1) != 0 {
            return None;
        }
    }
    Some(
        (0..bit_length)
            .map(|index| {
                let mask = 1 << (7 - index % 8);
                if bytes[4 + index / 8] & mask == 0 { '0' } else { '1' }
            })
            .collect(),
    )
}

fn decode_interval(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 16 {
        return None;
    }
    let microseconds = i64::from_be_bytes(bytes[0..8].try_into().ok()?);
    let days = i32::from_be_bytes(bytes[8..12].try_into().ok()?);
    let months = i32::from_be_bytes(bytes[12..16].try_into().ok()?);
    Some(format_interval(months, days, microseconds))
}

fn format_interval(months: i32, days: i32, microseconds: i64) -> String {
    let mut text = String::new();
    let mut is_zero = true;
    let mut is_before = false;
    for (count, unit) in [(months / 12, "year"), (months % 12, "mon"), (days, "day")] {
        if count == 0 {
            continue;
        }
        if !is_zero {
            text.push(' ');
        }
        if is_before && count > 0 {
            text.push('+');
        }
        let plural = if count == 1 { "" } else { "s" };
        text.push_str(&format!("{count} {unit}{plural}"));
        is_before = count < 0;
        is_zero = false;
    }
    if microseconds != 0 || is_zero {
        if !is_zero {
            text.push(' ');
        }
        if microseconds < 0 {
            text.push('-');
        } else if is_before {
            text.push('+');
        }
        if microseconds == i64::MIN {
            text.push_str("9223372036854.775808 seconds");
        } else {
            text.push_str(&crate::temporal::clock_text(microseconds.unsigned_abs()));
        }
    }
    text
}

fn decode_inet(bytes: &[u8], force_prefix: bool) -> Option<String> {
    let &[family, bits, flags, length, ..] = bytes else {
        return None;
    };
    let length = usize::from(length);
    if bytes.len() != 4 + length {
        return None;
    }
    let address = &bytes[4..];
    let (host, max_bits) = match family {
        2 => {
            let octets: [u8; 4] = address.try_into().ok()?;
            (std::net::Ipv4Addr::from(octets).to_string(), 32)
        }
        3 => {
            let octets: [u8; 16] = address.try_into().ok()?;
            (std::net::Ipv6Addr::from(octets).to_string(), 128)
        }
        _ => return None,
    };
    if u16::from(bits) > max_bits || flags > 1 {
        return None;
    }
    if force_prefix || u16::from(bits) != max_bits {
        Some(format!("{host}/{bits}"))
    } else {
        Some(host)
    }
}

fn decode_pg_lsn(bytes: &[u8]) -> Option<String> {
    let lsn = u64::from_be_bytes(bytes.try_into().ok()?);
    Some(format!("{:X}/{:X}", lsn >> 32, lsn as u32))
}

fn decode_tsvector(bytes: &[u8]) -> Option<String> {
    let (count, mut rest) = bytes.split_first_chunk::<4>()?;
    let mut lexemes = Vec::new();
    for _ in 0..u32::from_be_bytes(*count) {
        let end = rest.iter().position(|&byte| byte == 0)?;
        let word = std::str::from_utf8(&rest[..end]).ok()?;
        let (positions, tail) = rest[end + 1..].split_first_chunk::<2>()?;
        let count = usize::from(u16::from_be_bytes(*positions));
        if tail.len() < count * 2 {
            return None;
        }
        let (raw, tail) = tail.split_at(count * 2);
        rest = tail;
        lexemes.push(format_lexeme(word, raw));
    }
    rest.is_empty().then(|| lexemes.join(" "))
}

fn format_lexeme(word: &str, raw_positions: &[u8]) -> String {
    let quoted = word.replace('\\', "\\\\").replace('\'', "''");
    let positions: Vec<String> = raw_positions
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let packed = u16::from_be_bytes(*pair);
            let weight = match packed >> 14 {
                3 => "A",
                2 => "B",
                1 => "C",
                _ => "",
            };
            format!("{}{weight}", packed & 0x3fff)
        })
        .collect();
    if positions.is_empty() {
        format!("'{quoted}'")
    } else {
        format!("'{quoted}':{}", positions.join(","))
    }
}

fn decode_point(bytes: &[u8]) -> Option<String> {
    let pair = <[u8; 16]>::try_from(bytes).ok()?;
    let (x, y) = pair.split_at(8);
    let coordinate = |half: &[u8]| Some(float8_text(f64::from_be_bytes(half.try_into().ok()?)));
    Some(format!("({},{})", coordinate(x)?, coordinate(y)?))
}

pub(crate) fn float8_text(value: f64) -> String {
    float_text(
        value.is_finite().then(|| format!("{value:e}")),
        value.to_string(),
        value,
        15,
    )
}

pub(crate) fn float4_text(value: f32) -> String {
    float_text(
        value.is_finite().then(|| format!("{value:e}")),
        value.to_string(),
        f64::from(value),
        6,
    )
}

fn float_text(scientific: Option<String>, plain: String, value: f64, exponent_limit: i32) -> String {
    let Some(scientific) = scientific else {
        return if value.is_nan() {
            "NaN".into()
        } else if value > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    };
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return plain;
    };
    match exponent.parse::<i32>() {
        Ok(exponent) if exponent < -4 || exponent >= exponent_limit => {
            let sign = if exponent < 0 { '-' } else { '+' };
            format!("{mantissa}e{sign}{:02}", exponent.abs())
        }
        _ => plain,
    }
}

fn decode_int_range(bytes: &[u8], width: usize) -> Option<String> {
    let (&flags, mut rest) = bytes.split_first()?;
    if flags & 0x01 != 0 {
        return rest.is_empty().then(|| "empty".to_string());
    }
    let mut bound = |infinite: bool| -> Option<String> {
        if infinite {
            return Some(String::new());
        }
        let (length, tail) = rest.split_first_chunk::<4>()?;
        if u32::from_be_bytes(*length) as usize != width || tail.len() < width {
            return None;
        }
        let (value, tail) = tail.split_at(width);
        rest = tail;
        Some(if width == 4 {
            i32::from_be_bytes(value.try_into().ok()?).to_string()
        } else {
            i64::from_be_bytes(value.try_into().ok()?).to_string()
        })
    };
    let lower = bound(flags & 0x08 != 0)?;
    let upper = bound(flags & 0x10 != 0)?;
    if !rest.is_empty() {
        return None;
    }
    let open = if flags & 0x02 != 0 { '[' } else { '(' };
    let close = if flags & 0x04 != 0 { ']' } else { ')' };
    Some(format!("{open}{lower},{upper}{close}"))
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
