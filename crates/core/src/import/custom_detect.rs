use crate::export::CustomDelimiter;

const CANDIDATES: [&str; 5] = ["||", "::", "^^", "~~", "##"];
const SAMPLE_BYTES: usize = 64 * 1024;
const SAMPLE_LINES: usize = 20;
const MIN_LINES: usize = 2;

pub fn detect_custom_delimiter(bytes: &[u8]) -> Option<CustomDelimiter> {
    let head = &bytes[..bytes.len().min(SAMPLE_BYTES)];
    let (text, cut) = match std::str::from_utf8(head) {
        Ok(text) => (text, bytes.len() > SAMPLE_BYTES),
        Err(error) => (std::str::from_utf8(&head[..error.valid_up_to()]).ok()?, true),
    };
    let text = text.trim_start_matches('\u{feff}');
    if text.contains('"') {
        return None;
    }
    let mut lines: Vec<&str> = text.lines().collect();
    if cut {
        lines.pop();
    }
    lines.retain(|line| !line.is_empty());
    lines.truncate(SAMPLE_LINES);
    if lines.len() < MIN_LINES {
        return None;
    }
    CANDIDATES
        .iter()
        .find(|candidate| every_line_splits_only_on(&lines, candidate))
        .and_then(|candidate| CustomDelimiter::new(candidate).ok())
}

fn every_line_splits_only_on(lines: &[&str], candidate: &str) -> bool {
    let Some(character) = candidate.chars().next() else {
        return false;
    };
    let expected = lines[0].matches(candidate).count();
    expected > 0
        && lines.iter().all(|line| {
            line.matches(candidate).count() == expected
                && !line.replace(candidate, "").contains(character)
                && line.split(candidate).all(|field| !field.trim().is_empty())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected(text: &str) -> Option<String> {
        detect_custom_delimiter(text.as_bytes()).map(|delimiter| delimiter.as_str().to_string())
    }

    #[test]
    fn a_consistent_repeated_character_is_suggested() {
        assert_eq!(detected("id||name\n1||ada\n2||grace\n"), Some("||".into()));
        assert_eq!(detected("id::name::age\n1::ada::36\n"), Some("::".into()));
        assert_eq!(detected("\u{feff}a^^b\r\nc^^d\r\n"), Some("^^".into()));
    }

    #[test]
    fn ambiguous_or_ordinary_files_get_no_suggestion() {
        assert_eq!(detected("id|name\n1|ada\n"), None);
        assert_eq!(detected("id,name\n1,ada\n"), None);
        assert_eq!(detected("a||b\nc|d\n"), None);
        assert_eq!(detected("a||b\nc||d||e\n"), None);
        assert_eq!(detected("a|b||c\nd|e||f\n"), None);
        assert_eq!(detected("only||one line\n"), None);
        assert_eq!(detected(""), None);
        assert_eq!(detected("a;b\nc;d\n"), None);
    }

    #[test]
    fn empty_fields_quotes_and_lookalike_text_are_not_suggested() {
        assert_eq!(detected("a;;b\nc;;d\n"), None);
        assert_eq!(detected("a||\nb||\n"), None);
        assert_eq!(detected("## A\n## B\n"), None);
        assert_eq!(detected("::1\n::1\n"), None);
        assert_eq!(detected("\"a||b\",1\n\"c||d\",2\n"), None);
    }

    #[test]
    fn a_partial_last_line_after_invalid_utf8_is_ignored() {
        let mut bytes = b"a||b\nc||d\ne||".to_vec();
        bytes.push(0xff);
        assert_eq!(
            detect_custom_delimiter(&bytes).map(|d| d.as_str().to_string()),
            Some("||".into())
        );
    }

    #[test]
    fn a_sample_cut_inside_a_multibyte_character_still_detects() {
        let mut text = String::from("id||name\n");
        text.push_str(&"1||é\n".repeat(20_000));
        assert_eq!(
            detect_custom_delimiter(text.as_bytes()).map(|d| d.as_str().to_string()),
            Some("||".into())
        );
    }
}
