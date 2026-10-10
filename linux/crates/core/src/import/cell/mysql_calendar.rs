use chrono::{NaiveDate, NaiveTime};

pub(super) fn invalid_text(text: &str, data_type: &str) -> bool {
    let lowered = data_type.trim().to_ascii_lowercase();
    let date_text = if lowered == "date" {
        text
    } else if lowered.starts_with("datetime") {
        let Some((date, time)) = text.split_once(' ') else {
            return false;
        };
        if NaiveTime::parse_from_str(time, "%H:%M:%S%.f").is_err() {
            return false;
        }
        date
    } else {
        return false;
    };
    let mut parts = date_text.split('-');
    let (Some(year), Some(month), Some(day), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    if year.len() != 4 {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (year.parse(), month.parse(), day.parse()) else {
        return false;
    };
    (1..=12).contains(&month) && (1..=31).contains(&day) && NaiveDate::from_ymd_opt(year, month, day).is_none()
}
