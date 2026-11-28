use libduckdb_sys::v1::{duckdb_from_time_tz, duckdb_time_tz};

pub(crate) fn time_tz_to_string(value: duckdb_time_tz) -> String {
    let value = unsafe { duckdb_from_time_tz(value) };
    let time = value.time;
    let mut text = format!("{:02}:{:02}:{:02}", time.hour, time.min, time.sec);
    if time.micros != 0 {
        text.push('.');
        text.push_str(format!("{:06}", time.micros).trim_end_matches('0'));
    }
    let offset = value.offset;
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.unsigned_abs();
    text.push_str(&format!("{sign}{:02}:{:02}", offset / 3_600, offset % 3_600 / 60));
    if offset % 60 != 0 {
        text.push_str(&format!(":{:02}", offset % 60));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::time_tz_to_string;
    use libduckdb_sys::v1::duckdb_create_time_tz;

    #[test]
    fn timetz_text_keeps_fractional_seconds_and_second_offsets() {
        let value = unsafe { duckdb_create_time_tz(3_723_123_400, 20_712) };
        assert_eq!(time_tz_to_string(value), "01:02:03.1234+05:45:12");
    }

    #[test]
    fn timetz_text_keeps_the_end_of_day_and_negative_offset() {
        let value = unsafe { duckdb_create_time_tz(86_400_000_000, -57_599) };
        assert_eq!(time_tz_to_string(value), "24:00:00-15:59:59");
    }
}
