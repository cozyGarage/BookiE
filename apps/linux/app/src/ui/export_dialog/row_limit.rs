use relm4::adw::{self, prelude::*};

pub(crate) fn build() -> adw::EntryRow {
    adw::EntryRow::builder()
        .title(bookie_app_services::tr!("Maximum rows (optional)"))
        .build()
}

pub(super) fn parse(text: &str) -> Result<Option<usize>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    match text.parse::<usize>() {
        Ok(0) | Err(_) => Err(()),
        Ok(limit) => Ok(Some(limit)),
    }
}

pub(crate) fn choice(row: &adw::EntryRow) -> Result<Option<usize>, ()> {
    let parsed = parse(row.text().as_str());
    if parsed.is_err() {
        row.add_css_class("error");
        row.set_tooltip_text(Some(&bookie_app_services::tr!("Enter a whole number of at least 1")));
    } else {
        row.remove_css_class("error");
        row.set_tooltip_text(None);
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn empty_means_no_limit_and_only_positive_whole_numbers_are_limits() {
        assert_eq!(parse(""), Ok(None));
        assert_eq!(parse("  "), Ok(None));
        assert_eq!(parse(" 250 "), Ok(Some(250)));
        for bad in ["0", "-5", "1.5", "1e3", "ten", "99999999999999999999999"] {
            assert_eq!(parse(bad), Err(()), "{bad}");
        }
    }
}
