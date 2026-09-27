use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use duckdb::types::TimeUnit;
use tablepro_core::Value;

pub(crate) fn date(days: i32) -> Value {
    match days {
        i32::MAX => Value::Text("infinity".into()),
        value if value == -i32::MAX => Value::Text("-infinity".into()),
        _ => days
            .checked_add(719_163)
            .and_then(NaiveDate::from_num_days_from_ce_opt)
            .map(|date| {
                if (1..=9999).contains(&date.year()) {
                    Value::Date(date)
                } else {
                    Value::Text(calendar_text(date))
                }
            })
            .unwrap_or_else(|| Value::Undecodable("DATE outside supported calendar range".into())),
    }
}

fn calendar_text(date: NaiveDate) -> String {
    let (is_ce, year) = date.year_ce();
    format!(
        "{year:04}-{:02}-{:02}{}",
        date.month(),
        date.day(),
        if is_ce { "" } else { " (BC)" }
    )
}

fn seconds_and_nanos(unit: TimeUnit, value: i64) -> (i64, u32) {
    let units = match unit {
        TimeUnit::Second => 1,
        TimeUnit::Millisecond => 1_000,
        TimeUnit::Microsecond => 1_000_000,
        TimeUnit::Nanosecond => 1_000_000_000,
    };
    (
        value.div_euclid(units),
        (value.rem_euclid(units) * (1_000_000_000 / units)) as u32,
    )
}

pub(crate) fn timestamp(unit: TimeUnit, value: i64) -> Value {
    match value {
        i64::MAX => Value::Text("infinity".into()),
        value if value == -i64::MAX => Value::Text("-infinity".into()),
        _ => {
            let (seconds, nanos) = seconds_and_nanos(unit, value);
            DateTime::from_timestamp(seconds, nanos)
                .map(|date| {
                    if (1..=9999).contains(&date.year()) {
                        Value::DateTime(date.naive_utc())
                    } else {
                        Value::Undecodable("TIMESTAMP outside supported export calendar range".into())
                    }
                })
                .unwrap_or_else(|| Value::Undecodable("TIMESTAMP outside supported calendar range".into()))
        }
    }
}

pub(crate) fn time(unit: TimeUnit, value: i64) -> Value {
    let (seconds, nanos) = seconds_and_nanos(unit, value);
    if seconds == 86_400 && nanos == 0 {
        return Value::Text("24:00:00".into());
    }
    u32::try_from(seconds)
        .ok()
        .and_then(|seconds| NaiveTime::from_num_seconds_from_midnight_opt(seconds, nanos))
        .map(Value::Time)
        .unwrap_or_else(|| Value::Undecodable("TIME outside supported clock range".into()))
}

fn is_whole_micros(nanos: u32) -> bool {
    nanos < 1_000_000_000 && nanos.is_multiple_of(1_000)
}

pub(crate) fn date_param(date: NaiveDate) -> duckdb::types::Value {
    duckdb::types::Value::Date32(date.num_days_from_ce() - 719_163)
}

pub(crate) fn time_param(time: NaiveTime) -> duckdb::types::Value {
    if !is_whole_micros(time.nanosecond()) {
        return duckdb::types::Value::Text(time.to_string());
    }
    let micros = i64::from(time.num_seconds_from_midnight()) * 1_000_000 + i64::from(time.nanosecond() / 1_000);
    duckdb::types::Value::Time64(TimeUnit::Microsecond, micros)
}

pub(crate) fn timestamp_param(stamp: NaiveDateTime) -> duckdb::types::Value {
    let stamp = stamp.and_utc();
    if !is_whole_micros(stamp.timestamp_subsec_nanos()) {
        return duckdb::types::Value::Text(stamp.naive_utc().to_string());
    }
    duckdb::types::Value::Timestamp(TimeUnit::Microsecond, stamp.timestamp_micros())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_contract_temporal_units_and_negative_fractions_are_exact() {
        for (unit, scale) in [
            (TimeUnit::Second, 1),
            (TimeUnit::Millisecond, 1_000),
            (TimeUnit::Microsecond, 1_000_000),
            (TimeUnit::Nanosecond, 1_000_000_000),
        ] {
            assert_eq!(seconds_and_nanos(unit, 0), (0, 0));
            assert_eq!(seconds_and_nanos(unit, scale), (1, 0));
            assert_eq!(seconds_and_nanos(unit, -scale), (-1, 0));
            assert_eq!(
                seconds_and_nanos(unit, -1),
                (-1, (1_000_000_000 - 1_000_000_000 / scale) as u32)
            );
            assert_eq!(
                seconds_and_nanos(unit, 1),
                (1 / scale, if scale == 1 { 0 } else { (1_000_000_000 / scale) as u32 })
            );
            assert_eq!(timestamp(unit, i64::MAX), Value::Text("infinity".into()));
            assert_eq!(timestamp(unit, -i64::MAX), Value::Text("-infinity".into()));
            assert!(matches!(time(unit, -1), Value::Undecodable(_)));
            assert_eq!(time(unit, 86_400 * scale), Value::Text("24:00:00".into()));
            assert!(matches!(time(unit, 86_400 * scale + 1), Value::Undecodable(_)));
        }
    }

    #[test]
    fn value_contract_calendar_limits_refuse_overflow() {
        assert!(matches!(date(0), Value::Date(day) if day.to_string() == "1970-01-01"));
        for days in [i32::MIN, i32::MIN + 2, i32::MAX - 1] {
            assert!(matches!(date(days), Value::Undecodable(_)));
        }
        for value in [i64::MIN, i64::MAX - 1] {
            assert!(matches!(timestamp(TimeUnit::Second, value), Value::Undecodable(_)));
        }
    }
}
