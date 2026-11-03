use super::*;

fn store(directory: &tempfile::TempDir) -> SnippetStore {
    SnippetStore::load_at(directory.path().join("snippets.json"))
}

#[test]
fn a_snippet_survives_a_restart_and_lists_in_name_order() {
    let directory = tempfile::tempdir().unwrap();
    let snippets = store(&directory);
    snippets.save("sel", "SELECT * FROM $0 LIMIT 100;").unwrap();
    snippets.save("Count", "SELECT count(*) FROM $0;").unwrap();
    snippets.flush().unwrap();

    let reopened = store(&directory);
    let names: Vec<String> = reopened.list().into_iter().map(|snippet| snippet.name).collect();
    assert_eq!(names, ["Count", "sel"]);
    assert_eq!(reopened.get("SEL").unwrap().body, "SELECT * FROM $0 LIMIT 100;");
}

#[test]
fn saving_under_an_existing_name_replaces_it_whatever_the_case() {
    let directory = tempfile::tempdir().unwrap();
    let snippets = store(&directory);
    snippets.save("Top", "SELECT 1").unwrap();
    snippets.save("top", "SELECT 2").unwrap();
    let listed = snippets.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].body, "SELECT 2");
    assert_eq!(listed[0].name, "top");
}

#[test]
fn a_deleted_snippet_is_gone_after_a_restart() {
    let directory = tempfile::tempdir().unwrap();
    let snippets = store(&directory);
    snippets.save("a", "SELECT 1").unwrap();
    assert_eq!(snippets.delete("A"), Ok(true));
    assert_eq!(snippets.delete("a"), Ok(false));
    snippets.flush().unwrap();
    assert!(store(&directory).list().is_empty());
}

#[test]
fn names_and_bodies_outside_the_limits_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let snippets = store(&directory);
    for name in ["", "   ", "bad\nname", &"x".repeat(MAX_NAME_CHARS + 1)] {
        assert!(snippets.save(name, "SELECT 1").is_err(), "{name:?}");
    }
    assert!(snippets.save("empty", "  \n ").is_err());
    assert!(snippets.save("big", &"x".repeat(MAX_BODY_BYTES + 1)).is_err());
    assert!(snippets.list().is_empty());
}

#[test]
fn the_snippet_count_is_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let snippets = store(&directory);
    for index in 0..MAX_SNIPPETS {
        snippets.save(&format!("s{index}"), "SELECT 1").unwrap();
    }
    assert!(snippets.save("one too many", "SELECT 1").is_err());
    assert!(
        snippets.save("s0", "SELECT 2").is_ok(),
        "replacing an existing name is not growth"
    );
}

#[test]
fn hand_edited_entries_that_break_the_rules_are_not_offered() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snippets.json");
    std::fs::write(
        &path,
        r#"{"ok": {"name": "ok", "body": "SELECT 1"},
            "wrong-key": {"name": "Other", "body": "SELECT 2"},
            "empty": {"name": "empty", "body": ""}}"#,
    )
    .unwrap();
    let names: Vec<String> = SnippetStore::load_at(path).list().into_iter().map(|s| s.name).collect();
    assert_eq!(names, ["ok"]);
}

#[test]
fn expansion_puts_the_cursor_at_the_marker_and_removes_it() {
    let expansion = expand("SELECT * FROM $0 LIMIT 10;");
    assert_eq!(expansion.text, "SELECT * FROM  LIMIT 10;");
    assert_eq!(expansion.cursor, "SELECT * FROM ".len());
}

#[test]
fn expansion_without_a_marker_leaves_the_cursor_at_the_end() {
    let expansion = expand("SELECT 1;");
    assert_eq!((expansion.text.as_str(), expansion.cursor), ("SELECT 1;", 9));
}

#[test]
fn a_doubled_dollar_is_a_literal_dollar_and_only_the_first_marker_counts() {
    let expansion = expand("SELECT '$$5' $0 $0 $x");
    assert_eq!(expansion.text, "SELECT '$5'   $x");
    assert_eq!(expansion.cursor, "SELECT '$5' ".len());
}

#[test]
fn the_cursor_offset_is_a_byte_offset_on_a_character_boundary() {
    let expansion = expand("-- ünï\n$0");
    assert!(expansion.text.is_char_boundary(expansion.cursor));
    assert_eq!(expansion.cursor, "-- ünï\n".len());
}

#[test]
fn expansion_never_runs_anything_or_reads_the_environment() {
    let expansion = expand("$HOME ${USER} $(whoami) `id`");
    assert_eq!(expansion.text, "$HOME ${USER} $(whoami) `id`");
}
