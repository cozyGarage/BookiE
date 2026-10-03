use chrono::{NaiveTime, Timelike};

pub(super) fn duckdb_extended_date(text: &str) -> bool {
    let (date, bc) = text.strip_suffix(" (BC)").map_or((text, false), |date| (date, true));
    let mut parts = date.split('-');
    let Some(year_text) = parts.next().map(|year| year.strip_prefix('+').unwrap_or(year)) else {
        return false;
    };
    if year_text.len() < 4 || !year_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let Ok(year) = year_text.parse::<i64>() else {
        return false;
    };
    let Some(month) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
        return false;
    };
    let Some(day) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
        return false;
    };
    if parts.next().is_some() || year == 0 || month == 0 || month > 12 || (year <= 9999 && !bc) {
        return false;
    }
    let astronomical_year = if bc { 1 - year } else { year };
    let leap = astronomical_year % 4 == 0 && (astronomical_year % 100 != 0 || astronomical_year % 400 == 0);
    let last_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day > 0 && day <= last_day
}

pub fn duckdb_extended_timestamptz_nanos(text: &str) -> Option<u32> {
    let (date, time) = if let Some((date, time)) = text.split_once(" (BC) ") {
        (format!("{date} (BC)"), time)
    } else if let Some((date, time)) = text.split_once('T') {
        (date.to_owned(), time)
    } else {
        let (date, time) = text.split_once(' ')?;
        (date.to_owned(), time)
    };
    if !duckdb_extended_date(&date) {
        return None;
    }
    let zone_start = time.find(['+', '-', 'Z'])?;
    let (clock, zone) = time.split_at(zone_start);
    if !valid_duckdb_zone(zone) {
        return None;
    }
    let time = NaiveTime::parse_from_str(clock, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(clock, "%H:%M"))
        .ok()?;
    Some(time.nanosecond())
}

fn valid_duckdb_zone(zone: &str) -> bool {
    if zone == "Z" {
        return true;
    }
    let Some(digits) = zone.strip_prefix('+').or_else(|| zone.strip_prefix('-')) else {
        return false;
    };
    match digits.len() {
        2 => digits.parse::<u32>().is_ok_and(|hours| hours < 24),
        5 if digits.as_bytes().get(2) == Some(&b':') => digits[..2]
            .parse::<u32>()
            .ok()
            .zip(digits[3..].parse::<u32>().ok())
            .is_some_and(|(hours, minutes)| hours < 24 && minutes < 60),
        _ => false,
    }
}
