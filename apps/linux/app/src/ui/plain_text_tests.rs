use std::path::Path;

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
    let group = crate::ui::welcome_view::section_group(Some(HOSTILE), 0);
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
