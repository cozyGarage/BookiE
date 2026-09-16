pub(crate) fn decode_pg_binary_text(type_name: &str, bytes: &[u8]) -> Option<String> {
    match type_name {
        "INTERVAL" => decode_interval(bytes),
        "INET" => decode_inet(bytes, false),
        "CIDR" => decode_inet(bytes, true),
        "PG_LSN" => decode_pg_lsn(bytes),
        _ => None,
    }
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
    let years = months / 12;
    let months = months % 12;
    let mut parts = Vec::new();
    push_counted(&mut parts, years, "year", "years");
    push_counted(&mut parts, months, "mon", "mons");
    push_counted(&mut parts, days, "day", "days");
    if microseconds != 0 || parts.is_empty() {
        parts.push(format_interval_time(microseconds));
    }
    parts.join(" ")
}

fn push_counted(parts: &mut Vec<String>, count: i32, singular: &str, plural: &str) {
    if count == 0 {
        return;
    }
    let label = if count.abs() == 1 { singular } else { plural };
    parts.push(format!("{count} {label}"));
}

fn format_interval_time(microseconds: i64) -> String {
    let negative = microseconds < 0;
    let magnitude = microseconds.unsigned_abs();
    let hours = magnitude / 3_600_000_000;
    let remainder = magnitude % 3_600_000_000;
    let minutes = remainder / 60_000_000;
    let remainder = remainder % 60_000_000;
    let seconds = remainder / 1_000_000;
    let micros = remainder % 1_000_000;
    let sign = if negative { "-" } else { "" };
    if micros == 0 {
        format!("{sign}{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        let fraction = format!("{micros:06}");
        format!(
            "{sign}{hours:02}:{minutes:02}:{seconds:02}.{}",
            fraction.trim_end_matches('0')
        )
    }
}

fn decode_inet(bytes: &[u8], force_prefix: bool) -> Option<String> {
    if bytes.len() < 4 {
        return None;
    }
    let family = bytes[0];
    let bits = bytes[1];
    let length = usize::from(bytes[3]);
    if bytes.len() != 4 + length {
        return None;
    }
    let address = &bytes[4..];
    let (host, max_bits) = match family {
        2 if length == 4 => {
            let octets: [u8; 4] = address.try_into().ok()?;
            (std::net::Ipv4Addr::from(octets).to_string(), 32)
        }
        3 if length == 16 => {
            let octets: [u8; 16] = address.try_into().ok()?;
            (std::net::Ipv6Addr::from(octets).to_string(), 128)
        }
        _ => return None,
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn interval_bytes(months: i32, days: i32, microseconds: i64) -> [u8; 16] {
        let mut bytes = [0_u8; 16];
        bytes[0..8].copy_from_slice(&microseconds.to_be_bytes());
        bytes[8..12].copy_from_slice(&days.to_be_bytes());
        bytes[12..16].copy_from_slice(&months.to_be_bytes());
        bytes
    }

    #[test]
    fn interval_query_duration_matches_postgres_style() {
        assert_eq!(
            decode_pg_binary_text("INTERVAL", &interval_bytes(0, 0, 1_234_567)).as_deref(),
            Some("00:00:01.234567")
        );
        assert_eq!(
            decode_pg_binary_text("INTERVAL", &interval_bytes(0, 0, 3_723_000_000)).as_deref(),
            Some("01:02:03")
        );
        assert_eq!(
            decode_pg_binary_text("INTERVAL", &interval_bytes(0, 3, 0)).as_deref(),
            Some("3 days")
        );
        assert_eq!(
            decode_pg_binary_text(
                "INTERVAL",
                &interval_bytes(14, 3, 4 * 3_600_000_000 + 5 * 60_000_000 + 6_000_000)
            )
            .as_deref(),
            Some("1 year 2 mons 3 days 04:05:06")
        );
        assert_eq!(
            decode_pg_binary_text("INTERVAL", &interval_bytes(0, 0, 0)).as_deref(),
            Some("00:00:00")
        );
        assert_eq!(
            decode_pg_binary_text("INTERVAL", &interval_bytes(0, 0, -3_600_000_000)).as_deref(),
            Some("-01:00:00")
        );
    }

    #[test]
    fn inet_and_cidr_use_postgres_text() {
        let inet = [2, 32, 0, 4, 192, 0, 2, 1];
        assert_eq!(decode_pg_binary_text("INET", &inet).as_deref(), Some("192.0.2.1"));
        let cidr = [2, 24, 1, 4, 192, 0, 2, 0];
        assert_eq!(decode_pg_binary_text("CIDR", &cidr).as_deref(), Some("192.0.2.0/24"));
        let mut ipv6 = vec![3, 128, 0, 16];
        ipv6.extend_from_slice(&[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(decode_pg_binary_text("INET", &ipv6).as_deref(), Some("2001:db8::1"));
    }

    #[test]
    fn pg_lsn_uses_uppercase_hex() {
        let bytes = 0x0000_0000_016B_3748_u64.to_be_bytes();
        assert_eq!(decode_pg_binary_text("PG_LSN", &bytes).as_deref(), Some("0/16B3748"));
    }

    #[test]
    fn unknown_binary_types_stay_undecoded() {
        assert_eq!(decode_pg_binary_text("INT4RANGE", &[1, 2, 3, 4]), None);
        assert_eq!(decode_pg_binary_text("INTERVAL", &[0, 1, 2]), None);
    }
}
