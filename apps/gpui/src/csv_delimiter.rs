use bookie_core::export::{CustomDelimiter, CustomDelimiterError};

const COMMAND: &str = "csv delimiter";

pub type Parsed = (String, Result<CustomDelimiter, CustomDelimiterError>);

pub fn parse_command(typed: &str) -> Option<Parsed> {
    let head = typed.get(..COMMAND.len())?;
    if !head.eq_ignore_ascii_case(COMMAND) {
        return None;
    }
    let rest = typed[COMMAND.len()..].strip_prefix(' ')?;
    let delimiter = rest.trim_end_matches(['\n', '\r']);
    (!delimiter.is_empty()).then(|| (delimiter.to_string(), CustomDelimiter::new(delimiter)))
}

pub fn error_label(error: CustomDelimiterError) -> String {
    match error {
        CustomDelimiterError::Empty => "the delimiter is empty".into(),
        CustomDelimiterError::TooLong => "at most 8 characters".into(),
        CustomDelimiterError::LineBreak => "no line breaks".into(),
        CustomDelimiterError::Quote => "no quote characters".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_takes_the_text_after_one_space_and_validates_it() {
        let ok = |typed: &str| parse_command(typed).unwrap().1.unwrap().as_str().to_string();
        assert_eq!(ok("csv delimiter ||"), "||");
        assert_eq!(ok("CSV Delimiter ::"), "::");
        assert_eq!(ok("csv delimiter  |"), " |");
        assert_eq!(parse_command("csv delimiter 'x").unwrap().1, Err(CustomDelimiterError::Quote));
        assert_eq!(
            parse_command("csv delimiter 123456789").unwrap().1,
            Err(CustomDelimiterError::TooLong)
        );
    }

    #[test]
    fn other_text_is_not_the_command() {
        for typed in [
            "",
            "csv",
            "csv delimiter",
            "csv delimiter ",
            "export csv",
            "csv delimiters ||",
            "é",
        ] {
            assert!(parse_command(typed).is_none(), "{typed:?}");
        }
    }
}
