use chrono::NaiveTime;
use tablepro_core::Value;

const MICROS_PER_DAY: i64 = 86_400_000_000;
const OFFSET_LIMIT: i32 = 57_600;

pub(crate) fn decode_time(bytes: &[u8], with_zone: bool) -> Option<Value> {
    let expected = if with_zone { 12 } else { 8 };
    if bytes.len() != expected {
        return None;
    }
    let micros = i64::from_be_bytes(bytes[..8].try_into().ok()?);
    if !(0..=MICROS_PER_DAY).contains(&micros) {
        return None;
    }
    let time = if micros == MICROS_PER_DAY {
        Value::Text("24:00:00".into())
    } else {
        Value::Time(NaiveTime::from_num_seconds_from_midnight_opt(
            u32::try_from(micros / 1_000_000).ok()?,
            u32::try_from(micros % 1_000_000).ok()? * 1_000,
        )?)
    };
    if !with_zone {
        return Some(time);
    }
    let west = i32::from_be_bytes(bytes[8..].try_into().ok()?);
    if west <= -OFFSET_LIMIT || west >= OFFSET_LIMIT {
        return None;
    }
    let text = match time {
        Value::Time(time) => time.to_string(),
        Value::Text(text) => text,
        _ => return None,
    };
    let east = -west;
    let sign = if east < 0 { '-' } else { '+' };
    let offset = east.unsigned_abs();
    Some(Value::Text(format!(
        "{text}{sign}{:02}:{:02}:{:02}",
        offset / 3_600,
        offset % 3_600 / 60,
        offset % 60
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(micros: i64, west: i32) -> Vec<u8> {
        [micros.to_be_bytes().as_slice(), west.to_be_bytes().as_slice()].concat()
    }

    #[test]
    fn value_contract_time_keeps_end_of_day_and_microseconds() {
        for (micros, expected) in [
            (0, "00:00:00"),
            (1, "00:00:00.000001"),
            (45_296_123_456, "12:34:56.123456"),
            (MICROS_PER_DAY - 1, "23:59:59.999999"),
        ] {
            assert_eq!(
                decode_time(&micros.to_be_bytes(), false),
                Some(Value::Time(expected.parse().unwrap()))
            );
        }
        assert_eq!(
            decode_time(&MICROS_PER_DAY.to_be_bytes(), false),
            Some(Value::Text("24:00:00".into()))
        );
        for (west, expected) in [
            (0, "00:00:00+00:00:00"),
            (-20_712, "00:00:00+05:45:12"),
            (20_712, "00:00:00-05:45:12"),
            (-57_599, "00:00:00+15:59:59"),
            (57_599, "00:00:00-15:59:59"),
        ] {
            assert_eq!(decode_time(&wire(0, west), true), Some(Value::Text(expected.into())));
        }
        assert_eq!(
            decode_time(&wire(MICROS_PER_DAY, 0), true),
            Some(Value::Text("24:00:00+00:00:00".into()))
        );
    }

    #[test]
    fn value_contract_time_rejects_invalid_lengths_times_and_offsets() {
        for length in 0..24 {
            assert_eq!(decode_time(&vec![0; length], false).is_some(), length == 8);
            assert_eq!(decode_time(&vec![0; length], true).is_some(), length == 12);
        }
        for micros in [i64::MIN, -1, MICROS_PER_DAY + 1, i64::MAX] {
            assert_eq!(decode_time(&micros.to_be_bytes(), false), None);
            assert_eq!(decode_time(&wire(micros, 0), true), None);
        }
        for west in [i32::MIN, -OFFSET_LIMIT, OFFSET_LIMIT, i32::MAX] {
            assert_eq!(decode_time(&wire(0, west), true), None);
        }
    }
}
