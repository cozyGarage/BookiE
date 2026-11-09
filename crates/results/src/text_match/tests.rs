use super::*;

fn matcher(term: &str, options: TextMatchOptions) -> TextMatcher {
    TextMatcher::compile(term, options).unwrap().expect("matcher")
}

#[test]
fn bounded_replacement_observes_cancellation_between_matches() {
    let checks = std::cell::Cell::new(0);
    let result = matcher("a", TextMatchOptions::default()).replace_all_bounded_controlled("aaaa", "b", 100, || {
        checks.set(checks.get() + 1);
        checks.get() >= 3
    });
    assert_eq!(result, Err(ReplacementLimit::Cancelled));
}

#[test]
fn bounded_replacement_observes_cancellation_while_lowercasing_unicode() {
    let checks = std::cell::Cell::new(0);
    let result = matcher("x", TextMatchOptions::default()).replace_all_bounded_controlled(
        &format!("İ{}", "x".repeat(100)),
        "y",
        1024,
        || {
            checks.set(checks.get() + 1);
            checks.get() >= 4
        },
    );
    assert_eq!(result, Err(ReplacementLimit::Cancelled));
}

#[test]
fn bounded_replacement_refuses_literal_expansion_before_returning_output() {
    let matcher = matcher("a", TextMatchOptions::default());
    assert!(matcher.replace_all_bounded("aaaa", "xxxx", 15).is_err());
    assert_eq!(
        matcher.replace_all_bounded("aaaa", "xxxx", 16).unwrap(),
        ("x".repeat(16), 4)
    );
}

#[test]
fn bounded_replacement_counts_capture_expansion_in_utf8_bytes() {
    let matcher = matcher(
        "(?P<word>é)",
        TextMatchOptions {
            regex: true,
            ..Default::default()
        },
    );
    assert!(matcher.replace_all_bounded("é", "${word}$0", 3).is_err());
    assert_eq!(
        matcher.replace_all_bounded("é", "${word}$0", 4).unwrap(),
        ("éé".into(), 1)
    );
}

#[test]
fn bounded_replacement_preserves_existing_template_and_unicode_semantics() {
    let cases = [
        ("(?P<word>é)|(a)", "é a", "${word}:$0:$2:$$:$missing:${missing}:$"),
        ("(a)", "aba", "$1x-${1}x-${1-$999999999999999999999999999999"),
        ("a*", "baac", "[$0]"),
        ("(?P<x>a)", "a", "${x}é${}$$$x"),
    ];
    for (pattern, text, template) in cases {
        let matcher = matcher(
            pattern,
            TextMatchOptions {
                regex: true,
                ..Default::default()
            },
        );
        let expected = matcher.replace_all(text, template);
        assert_eq!(
            matcher.replace_all_bounded(text, template, expected.0.len()).unwrap(),
            expected
        );
        if !expected.0.is_empty() {
            assert!(
                matcher
                    .replace_all_bounded(text, template, expected.0.len() - 1)
                    .is_err()
            );
        }
    }
    for text in ["İ École Élite", "İ i", "K k", "é É"] {
        for needle in ["i", "é", "k"] {
            let matcher = matcher(needle, TextMatchOptions::default());
            assert_eq!(
                matcher.replace_all_bounded(text, "$0", 1024).unwrap(),
                matcher.replace_all(text, "$0")
            );
        }
    }
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
    assert!(
        TextMatcher::compile("   ", TextMatchOptions::default())
            .unwrap()
            .is_none()
    );
    assert!(
        TextMatcher::compile(
            "",
            TextMatchOptions {
                regex: true,
                ..Default::default()
            }
        )
        .unwrap()
        .is_none()
    );
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

#[test]
fn case_insensitive_replace_maps_expanding_lowercase_to_source_span() {
    let matcher = matcher("é", TextMatchOptions::default());
    assert_eq!(matcher.replace_all("İ École", "X"), ("İ Xcole".to_owned(), 1));
}

#[test]
fn case_insensitive_replace_keeps_matches_after_expanding_lowercase() {
    let matcher = matcher("é", TextMatchOptions::default());
    assert_eq!(
        matcher.replace_all("İ École Élite", "X"),
        ("İ Xcole Xlite".to_owned(), 2)
    );
}

#[test]
fn case_insensitive_ascii_search_keeps_expanding_unicode_matches_in_source_order() {
    let matcher = matcher("i", TextMatchOptions::default());
    assert_eq!(matcher.replace_all("İ i", "X"), ("X X".to_owned(), 2));
}

#[test]
fn case_insensitive_replacement_maps_shorter_lowercase_to_source_span() {
    let matcher = matcher("k", TextMatchOptions::default());
    assert_eq!(matcher.replace_all("K k", "X"), ("X X".to_owned(), 2));
}

#[test]
fn case_insensitive_replacement_preserves_many_ascii_and_unicode_matches() {
    let repeated = "x".repeat(15_000);
    let matcher = matcher("x", TextMatchOptions::default());

    for text in [repeated.clone(), format!("İ{repeated}")] {
        let expected = text.replacen('x', "y", 15_000);
        assert_eq!(matcher.replace_all(&text, "y"), (expected.clone(), 15_000));
        assert_eq!(
            matcher.replace_all_bounded(&text, "y", expected.len()).unwrap(),
            (expected, 15_000)
        );
    }
}
