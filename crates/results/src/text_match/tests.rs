use super::*;

fn matcher(term: &str, options: TextMatchOptions) -> TextMatcher {
    TextMatcher::compile(term, options).unwrap().expect("matcher")
}

#[test]
fn literal_search_is_case_insensitive_by_default() {
    let options = TextMatchOptions::default();
    assert!(matcher("World", options).matches("Hello World"));
    assert!(matcher("WORLD", options).matches("Hello World"));
    assert!(!matcher("planet", options).matches("Hello World"));
}

#[test]
fn match_case_requires_exact_letter_case() {
    let options = TextMatchOptions {
        match_case: true,
        ..TextMatchOptions::default()
    };
    assert!(matcher("World", options).matches("Hello World"));
    assert!(!matcher("world", options).matches("Hello World"));
}

#[test]
fn whole_words_reject_embedded_matches() {
    let options = TextMatchOptions {
        whole_words: true,
        ..TextMatchOptions::default()
    };
    assert!(matcher("cat", options).matches("the cat sat"));
    assert!(!matcher("cat", options).matches("concatenate"));
}

#[test]
fn regex_matches_and_rejects_invalid_patterns() {
    let options = TextMatchOptions {
        regex: true,
        ..TextMatchOptions::default()
    };
    assert!(matcher(r"c.t", options).matches("cat"));
    assert_eq!(
        TextMatcher::compile(r"(", options).unwrap_err(),
        TextMatchError::InvalidPattern
    );
}

#[test]
fn empty_terms_compile_to_no_matcher() {
    assert!(TextMatcher::compile("   ", TextMatchOptions::default()).unwrap().is_none());
    assert!(TextMatcher::compile("", TextMatchOptions { regex: true, ..Default::default() })
        .unwrap()
        .is_none());
}

#[test]
fn replace_all_rewrites_every_literal_match() {
    let options = TextMatchOptions::default();
    let (text, count) = matcher("ab", options).replace_all("abXab", "!");
    assert_eq!((text.as_str(), count), ("!X!", 2));
}

#[test]
fn replace_all_expands_regex_capture_groups() {
    let options = TextMatchOptions {
        regex: true,
        ..TextMatchOptions::default()
    };
    let (text, count) = matcher(r"(\w+)=(\w+)", options).replace_all("a=b", "$2=$1");
    assert_eq!((text.as_str(), count), ("b=a", 1));
}

#[test]
fn whole_word_replace_does_not_expand_dollar_templates() {
    let options = TextMatchOptions {
        whole_words: true,
        ..TextMatchOptions::default()
    };
    let (text, count) = matcher("id", options).replace_all("id=1", "$0");
    assert_eq!((text.as_str(), count), ("$0=1", 1));
}
