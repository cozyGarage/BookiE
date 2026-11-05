use std::path::{Path, PathBuf};

use relm4::adw::prelude::*;
use relm4::gtk;

use super::*;

const HOSTILE: &str = "R&D <b>x</b> a < b";

fn displayed_texts(widget: &gtk::Widget) -> Vec<String> {
    let mut texts = Vec::new();
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        texts.push(label.text().to_string());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        texts.extend(displayed_texts(&current));
        child = current.next_sibling();
    }
    texts
}

fn shows_literally(widget: &impl IsA<gtk::Widget>, text: &str) -> bool {
    displayed_texts(widget.upcast_ref()).iter().any(|shown| shown == text)
}

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "tests") {
                rust_sources(&path, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

fn production_sources() -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    rust_sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut found);
    found
        .into_iter()
        .filter(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            name != "plain_text.rs" && name != "tests.rs" && !name.ends_with("_tests.rs")
        })
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect()
}

#[test]
fn escaped_text_cannot_inject_markup() {
    assert_eq!(escape_markup("plain"), "plain");
    assert_eq!(escape_markup("a < b & c"), "a &lt; b &amp; c");
    assert_eq!(
        escape_markup("<a href=\"https://example.com\">click</a>"),
        "&lt;a href=&quot;https://example.com&quot;&gt;click&lt;/a&gt;"
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_toast_shows_its_message_literally() {
    adw::init().unwrap();
    for text in [
        "R&D imported",
        "<b>bold</b>",
        "<a href=\"https://example.com\">link</a>",
        "a < b",
    ] {
        let toast = toast(text);
        assert!(!toast.uses_markup(), "{text}");
        assert_eq!(toast.title().as_deref(), Some(text));
    }
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_row_without_markup_shows_a_hostile_title_and_subtitle_literally() {
    adw::init().unwrap();
    let row = adw::ActionRow::builder()
        .use_markup(false)
        .title(HOSTILE)
        .subtitle(HOSTILE)
        .build();
    let shown = displayed_texts(row.upcast_ref());
    assert_eq!(
        shown.iter().filter(|text| text.as_str() == HOSTILE).count(),
        2,
        "{shown:?}"
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_row_left_with_default_markup_does_not_show_the_same_text() {
    adw::init().unwrap();
    let row = adw::ActionRow::builder().title(HOSTILE).subtitle(HOSTILE).build();
    assert!(
        !shows_literally(&row, HOSTILE),
        "default markup unexpectedly shows the text literally; the tests above prove nothing"
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_banner_without_markup_shows_a_hostile_title_literally() {
    adw::init().unwrap();
    let banner = adw::Banner::builder().use_markup(false).title(HOSTILE).build();
    assert!(
        shows_literally(&banner, HOSTILE),
        "{:?}",
        displayed_texts(banner.upcast_ref())
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_status_page_description_through_the_helper_shows_server_text_literally() {
    adw::init().unwrap();
    let page = adw::StatusPage::builder().description(escape_markup(HOSTILE)).build();
    assert!(
        shows_literally(&page, HOSTILE),
        "{:?}",
        displayed_texts(page.upcast_ref())
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_status_page_description_without_the_helper_does_not_show_the_same_text() {
    adw::init().unwrap();
    let page = adw::StatusPage::builder().description(HOSTILE).build();
    assert!(
        !shows_literally(&page, HOSTILE),
        "an unescaped description unexpectedly shows literally; the helper test proves nothing"
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_preferences_group_with_an_escaped_title_and_description_shows_both_literally() {
    adw::init().unwrap();
    let group = adw::PreferencesGroup::builder()
        .title(escape_markup(HOSTILE))
        .description(escape_markup(HOSTILE))
        .build();
    let shown = displayed_texts(group.upcast_ref());
    assert_eq!(
        shown.iter().filter(|text| text.as_str() == HOSTILE).count(),
        2,
        "{shown:?}"
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_preferences_group_left_unescaped_loses_a_title_with_markup_characters() {
    adw::init().unwrap();
    let group = adw::PreferencesGroup::builder()
        .title(HOSTILE)
        .description(HOSTILE)
        .build();
    assert!(
        !shows_literally(&group, HOSTILE),
        "an unescaped group unexpectedly shows literally; the escaped-group test proves nothing"
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_welcome_screen_group_named_with_markup_characters_shows_its_name() {
    adw::init().unwrap();
    let group = crate::ui::welcome_view::section_group(Some(HOSTILE));
    assert!(
        shows_literally(&group, HOSTILE),
        "{:?}",
        displayed_texts(group.upcast_ref())
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_import_file_group_shows_a_path_with_markup_characters_literally() {
    adw::init().unwrap();
    let path = "/tmp/R&D <x>/a & b.csv";
    let group = crate::ui::import_dialog::file_group(Path::new(path));
    assert!(
        shows_literally(&group, path),
        "{:?}",
        displayed_texts(group.upcast_ref())
    );
}

fn call_argument(text: &str, call: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(position) = text[from..].find(call) {
        let open = from + position + call.len();
        let mut depth = 1;
        let mut end = open;
        for (offset, character) in text[open..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                end = open + offset;
                break;
            }
        }
        found.push(text[open..end].trim().to_string());
        from = open;
    }
    found
}

fn is_plain_or_protected(argument: &str) -> bool {
    argument.starts_with('"')
        || argument.starts_with("crate::tr!")
        || argument.starts_with("tr!")
        || argument.starts_with("&crate::tr!")
        || argument.starts_with("&tr!")
        || argument.contains("escape_markup(")
        || argument.contains("markup_escape_text(")
        || argument.contains("glib_escaped(")
}

fn statements(text: &str, start: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(position) = text[from..].find(start) {
        let begin = from + position;
        let rest = &text[begin..];
        let end = rest.find(".build()").unwrap_or_else(|| rest.len().min(900));
        found.push(rest[..end].to_string());
        from = begin + start.len();
    }
    found
}

#[test]
fn a_row_builder_with_dynamic_text_turns_markup_off_or_escapes_it() {
    let kinds = [
        "ActionRow",
        "ExpanderRow",
        "SwitchRow",
        "ComboRow",
        "EntryRow",
        "PasswordEntryRow",
    ];
    let mut offenders = Vec::new();
    for (path, text) in production_sources() {
        for kind in kinds {
            for statement in statements(&text, &format!("adw::{kind}::builder()")) {
                let dynamic = ["subtitle(", "title("]
                    .iter()
                    .flat_map(|call| call_argument(&statement, &format!(".{call}")))
                    .any(|argument| !is_plain_or_protected(&argument));
                if dynamic && !statement.contains("use_markup(false)") {
                    offenders.push(format!(
                        "{}: {}",
                        path.display(),
                        statement.lines().take(3).collect::<Vec<_>>().join(" ")
                    ));
                }
            }
        }
    }
    assert!(offenders.is_empty(), "rows parse their text as markup: {offenders:#?}");
}

#[test]
fn a_status_page_or_group_description_with_dynamic_text_is_escaped() {
    let mut offenders = Vec::new();
    for (path, text) in production_sources() {
        for call in [".description(", ".set_description(Some("] {
            for argument in call_argument(&text, call) {
                if !is_plain_or_protected(argument.trim_start_matches('&')) && !is_plain_or_protected(&argument) {
                    offenders.push(format!("{}: {call}{argument}", path.display()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "descriptions parse their text as markup: {offenders:#?}"
    );
}

#[test]
fn every_banner_turns_markup_off() {
    let mut offenders = Vec::new();
    for (path, text) in production_sources() {
        for statement in statements(&text, "adw::Banner::builder()") {
            if !statement.contains("use_markup(false)") {
                offenders.push(format!("{}: builder", path.display()));
            }
        }
        let mut from = 0;
        while let Some(position) = text[from..].find("adw::Banner::new(") {
            let begin = from + position;
            let window: String = text[begin..].chars().take(160).collect();
            if !window.contains("set_use_markup(false)") {
                offenders.push(format!("{}: new", path.display()));
            }
            from = begin + 1;
        }
        let mut from = 0;
        while let Some(position) = text[from..].find("&adw::Banner {") {
            let begin = from + position;
            let block: String = text[begin..].chars().take(260).collect();
            if !block.contains("set_use_markup: false") {
                offenders.push(format!("{}: view", path.display()));
            }
            from = begin + 1;
        }
    }
    assert!(
        offenders.is_empty(),
        "banners parse their title as markup: {offenders:#?}"
    );
}

#[test]
fn every_toast_is_built_by_the_plain_text_helper() {
    let offenders: Vec<String> = production_sources()
        .into_iter()
        .filter(|(_, text)| text.contains("Toast::new(") || text.contains("Toast::builder("))
        .map(|(path, _)| path.display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "toast text is parsed as markup unless built with ui::plain_text::toast: {offenders:?}"
    );
}
