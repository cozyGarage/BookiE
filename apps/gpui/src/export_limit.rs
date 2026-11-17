const COMMAND: &str = "export limit";

pub type Parsed = (String, Result<usize, ()>);

pub fn parse_command(typed: &str) -> Option<Parsed> {
    let head = typed.get(..COMMAND.len())?;
    if !head.eq_ignore_ascii_case(COMMAND) {
        return None;
    }
    let rest = typed[COMMAND.len()..].strip_prefix(' ')?.trim();
    (!rest.is_empty()).then(|| (rest.to_string(), parse_limit(rest)))
}

fn parse_limit(text: &str) -> Result<usize, ()> {
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    match text.parse::<usize>() {
        Ok(0) | Err(_) => Err(()),
        Ok(limit) => Ok(limit),
    }
}

pub fn error_label() -> &'static str {
    "a whole number of at least 1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_takes_a_positive_whole_number() {
        assert_eq!(parse_command("export limit 250").unwrap().1, Ok(250));
        assert_eq!(parse_command("Export Limit  7 ").unwrap().1, Ok(7));
        for bad in ["0", "-1", "1.5", "ten", "99999999999999999999999"] {
            assert_eq!(parse_command(&format!("export limit {bad}")).unwrap().1, Err(()), "{bad}");
        }
    }

    #[test]
    fn other_text_is_not_the_command() {
        for typed in ["", "export", "export limit", "export limit ", "export csv", "export limits 5", "é"] {
            assert!(parse_command(typed).is_none(), "{typed:?}");
        }
    }
}
