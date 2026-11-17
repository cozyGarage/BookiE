use bookie_core::export::{CustomDelimiter, CustomDelimiterError};

pub(crate) fn parse(text: &str) -> Result<Option<CustomDelimiter>, CustomDelimiterError> {
    if text.is_empty() {
        return Ok(None);
    }
    CustomDelimiter::new(text).map(Some)
}

pub(crate) fn message(error: CustomDelimiterError) -> String {
    match error {
        CustomDelimiterError::Empty => bookie_app_services::tr!("The delimiter is empty."),
        CustomDelimiterError::TooLong => bookie_app_services::tr!("The delimiter is too long (at most 8 characters)."),
        CustomDelimiterError::LineBreak => bookie_app_services::tr!("The delimiter cannot contain a line break."),
        CustomDelimiterError::Quote => bookie_app_services::tr!("The delimiter cannot contain a quote character."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_means_no_custom_delimiter_and_bad_text_is_refused() {
        assert_eq!(parse(""), Ok(None));
        assert_eq!(parse("||").unwrap().map(|d| d.as_str().to_string()), Some("||".into()));
        assert_eq!(parse("a'b"), Err(CustomDelimiterError::Quote));
        assert_eq!(parse("a\nb"), Err(CustomDelimiterError::LineBreak));
        assert_eq!(parse("123456789"), Err(CustomDelimiterError::TooLong));
        assert!(!message(CustomDelimiterError::Quote).is_empty());
    }
}
