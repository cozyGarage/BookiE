use bookie_core::Value;

use crate::preferences::{DateFormatPref, NullDisplayPref};

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayPrefs {
    pub null_display: NullDisplayPref,
    pub date_format: DateFormatPref,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintedText(String);

impl PaintedText {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyText(String);

impl CopyText {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

pub fn paint_value(value: &Value, prefs: DisplayPrefs) -> PaintedText {
    PaintedText(format_value(value, prefs, TextRole::Paint))
}

pub fn copy_text(value: &Value) -> CopyText {
    CopyText(format_value(value, DisplayPrefs::default(), TextRole::Copy))
}

#[derive(Clone, Copy)]
enum TextRole {
    Paint,
    Copy,
}

fn format_value(value: &Value, prefs: DisplayPrefs, role: TextRole) -> String {
    match value {
        Value::Null => match role {
            TextRole::Paint => null_paint(prefs.null_display),
            TextRole::Copy => String::new(),
        },
        Value::Bool(flag) => flag.to_string(),
        Value::Int(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        Value::Text(text) => text.clone(),
        Value::Bytes(bytes) => bytes_text(bytes),
        Value::Date(_) | Value::DateTime(_) | Value::TimestampTz(_) => {
            let format = match role {
                TextRole::Paint => prefs.date_format,
                TextRole::Copy => DateFormatPref::Iso,
            };
            format_temporal_for_display(value, format).unwrap_or_default()
        }
        Value::Time(time) => time.format("%H:%M:%S").to_string(),
        Value::Decimal(number) => number.to_string(),
        Value::Uuid(id) => id.to_string(),
        Value::Json(json) => json.to_string(),
        Value::JsonText(text) => text.clone(),
        Value::Undecodable(_) if value.is_missing_field() => "(missing)".to_owned(),
        Value::Undecodable(type_name) => format!("<undecodable {type_name}>"),
    }
}

fn null_paint(style: NullDisplayPref) -> String {
    match style {
        NullDisplayPref::Standard => "NULL".to_owned(),
        NullDisplayPref::Symbol => "\u{2205}".to_owned(),
        NullDisplayPref::Parenthesized => "(null)".to_owned(),
    }
}

fn bytes_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => text.to_owned(),
        _ => format!("<{} bytes>", bytes.len()),
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

    #[test]
    fn paint_value_uses_a_non_default_date_form() {
        let day = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let stamp = Value::DateTime(day.and_hms_opt(23, 59, 58).unwrap());
        let painted = paint_value(
            &stamp,
            DisplayPrefs {
                date_format: DateFormatPref::UsShort,
                ..DisplayPrefs::default()
            },
        );
        assert_eq!(painted.as_str(), "12/31/2024");
    }

    #[test]
    fn copy_text_ignores_display_date_form() {
        let day = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let stamp = Value::DateTime(day.and_hms_opt(23, 59, 58).unwrap());
        let prefs = DisplayPrefs {
            date_format: DateFormatPref::UsShort,
            ..DisplayPrefs::default()
        };
        let painted = paint_value(&stamp, prefs);
        let copied = copy_text(&stamp);
        assert_eq!(painted.as_str(), "12/31/2024");
        assert_eq!(copied.as_str(), "2024-12-31 23:59:58");
        assert_ne!(painted.as_str(), copied.as_str());
    }

    #[test]
    fn null_paint_follows_pref_while_copy_stays_empty() {
        let value = Value::Null;
        let painted = paint_value(
            &value,
            DisplayPrefs {
                null_display: NullDisplayPref::Symbol,
                ..DisplayPrefs::default()
            },
        );
        assert_eq!(painted.as_str(), "\u{2205}");
        assert_eq!(copy_text(&value).as_str(), "");
    }

    #[test]
    fn paint_does_not_mutate_the_source_value() {
        let stamp = Value::DateTime(
            NaiveDate::from_ymd_opt(2024, 12, 31)
                .unwrap()
                .and_hms_opt(23, 59, 58)
                .unwrap(),
        );
        let before = stamp.clone();
        let _ = paint_value(
            &stamp,
            DisplayPrefs {
                date_format: DateFormatPref::EuShort,
                ..DisplayPrefs::default()
            },
        );
        let _ = copy_text(&stamp);
        assert_eq!(stamp, before);
    }
}
