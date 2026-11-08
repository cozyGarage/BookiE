use bookie_core::Value;

use crate::preferences::DateFormatPref;

impl DateFormatPref {
    fn clock_pattern(self) -> Option<&'static str> {
        match self {
            Self::Iso => None,
            Self::IsoDate => Some("%Y-%m-%d"),
            Self::UsLong => Some("%m/%d/%Y %I:%M:%S%.f %p"),
            Self::UsShort => Some("%m/%d/%Y"),
            Self::EuLong => Some("%d/%m/%Y %H:%M:%S%.f"),
            Self::EuShort => Some("%d/%m/%Y"),
        }
    }

    fn day_pattern(self) -> Option<&'static str> {
        match self {
            Self::Iso => None,
            Self::IsoDate => Some("%Y-%m-%d"),
            Self::UsLong | Self::UsShort => Some("%m/%d/%Y"),
            Self::EuLong | Self::EuShort => Some("%d/%m/%Y"),
        }
    }
}

pub fn format_temporal_for_display(value: &Value, format: DateFormatPref) -> Option<String> {
    match value {
        Value::Date(day) => Some(match format.day_pattern() {
            Some(pattern) => day.format(pattern).to_string(),
            None => day.format("%Y-%m-%d").to_string(),
        }),
        Value::DateTime(at) => Some(match format.clock_pattern() {
            Some(pattern) => at.format(pattern).to_string(),
            None => at.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
        }),
        Value::TimestampTz(at) => Some(match format.clock_pattern() {
            Some(pattern) => {
                format!("{} {}", at.naive_utc().format(pattern), at.format("%:z"))
            }
            None => at.format("%Y-%m-%d %H:%M:%S%:z").to_string(),
        }),
        _ => None,
    }
}

pub fn format_temporal_for_edit(value: &Value) -> Option<String> {
    format_temporal_for_display(value, DateFormatPref::Iso)
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};

    use super::*;

    #[test]
    fn dates_follow_the_chosen_format_and_iso_leaves_them_alone() {
        let day = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let at: NaiveDateTime = day.and_hms_opt(23, 59, 58).unwrap();
        let date = Value::Date(day);
        let stamp = Value::DateTime(at);

        assert_eq!(
            format_temporal_for_display(&date, DateFormatPref::Iso).as_deref(),
            Some("2024-12-31")
        );
        assert_eq!(
            format_temporal_for_display(&stamp, DateFormatPref::Iso).as_deref(),
            Some("2024-12-31 23:59:58")
        );
        for (format, expected_day, expected_stamp) in [
            (DateFormatPref::UsLong, "12/31/2024", "12/31/2024 11:59:58 PM"),
            (DateFormatPref::UsShort, "12/31/2024", "12/31/2024"),
            (DateFormatPref::EuLong, "31/12/2024", "31/12/2024 23:59:58"),
            (DateFormatPref::EuShort, "31/12/2024", "31/12/2024"),
            (DateFormatPref::IsoDate, "2024-12-31", "2024-12-31"),
        ] {
            assert_eq!(
                format_temporal_for_display(&date, format).as_deref(),
                Some(expected_day),
                "{format:?}"
            );
            assert_eq!(
                format_temporal_for_display(&stamp, format).as_deref(),
                Some(expected_stamp),
                "{format:?}"
            );
        }
    }

    #[test]
    fn a_timestamp_with_an_offset_keeps_its_offset_in_every_format() {
        let at = Utc.with_ymd_and_hms(2024, 12, 31, 23, 59, 58).unwrap();
        let value = Value::TimestampTz(at);
        assert_eq!(
            format_temporal_for_display(&value, DateFormatPref::Iso).as_deref(),
            Some("2024-12-31 23:59:58+00:00")
        );
        assert_eq!(
            format_temporal_for_display(&value, DateFormatPref::EuLong).as_deref(),
            Some("31/12/2024 23:59:58 +00:00")
        );
        assert_eq!(
            format_temporal_for_display(&value, DateFormatPref::UsShort).as_deref(),
            Some("12/31/2024 +00:00")
        );
    }

    #[test]
    fn edit_and_copy_keep_the_database_spelling_when_display_differs() {
        let day = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let at = day.and_hms_opt(23, 59, 58).unwrap();
        let date = Value::Date(day);
        let stamp = Value::DateTime(at);
        let painted_date = format_temporal_for_display(&date, DateFormatPref::EuShort).unwrap();
        let painted_stamp = format_temporal_for_display(&stamp, DateFormatPref::UsLong).unwrap();
        assert_eq!(format_temporal_for_edit(&date).as_deref(), Some("2024-12-31"));
        assert_eq!(format_temporal_for_edit(&stamp).as_deref(), Some("2024-12-31 23:59:58"));
        assert_ne!(painted_date, format_temporal_for_edit(&date).unwrap());
        assert_ne!(painted_stamp, format_temporal_for_edit(&stamp).unwrap());
    }

    #[test]
    fn non_temporal_values_are_left_to_the_caller() {
        assert_eq!(
            format_temporal_for_display(&Value::Text("2024-12-31".into()), DateFormatPref::EuShort),
            None
        );
        assert_eq!(
            format_temporal_for_display(
                &Value::Time(chrono::NaiveTime::from_hms_opt(14, 30, 0).unwrap()),
                DateFormatPref::UsLong
            ),
            None
        );
    }
}
