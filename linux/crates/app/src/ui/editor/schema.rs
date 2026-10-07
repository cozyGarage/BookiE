use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use sourceview5::prelude::*;

pub const SQL_KEYWORDS: &str = "\
SELECT FROM WHERE INSERT INTO VALUES UPDATE SET DELETE \
JOIN INNER LEFT RIGHT FULL OUTER ON USING UNION INTERSECT EXCEPT \
GROUP BY ORDER HAVING LIMIT OFFSET DISTINCT ALL AS WITH \
CREATE TABLE INDEX VIEW DROP ALTER TRUNCATE \
PRIMARY KEY FOREIGN REFERENCES UNIQUE NOT NULL DEFAULT CHECK \
AND OR IS LIKE IN BETWEEN EXISTS ANY \
COUNT SUM AVG MIN MAX CASE WHEN THEN ELSE END \
TRUE FALSE ASC DESC RETURNING";

pub fn build_schema_buffer() -> gtk::TextBuffer {
    let buf = gtk::TextBuffer::new(None);
    buf.set_text(SQL_KEYWORDS);
    buf
}

pub fn update_schema_buffer(buffer: &gtk::TextBuffer, schema_words: &[String]) {
    let mut text = String::from(SQL_KEYWORDS);
    for w in schema_words {
        text.push(' ');
        text.push_str(w);
    }
    buffer.set_text(&text);
}

pub fn derive_tab_label(query: &str) -> String {
    for line in query.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        let cleaned: String = trimmed.chars().take(30).collect();
        if cleaned.chars().count() < trimmed.chars().count() {
            return format!("{cleaned}…");
        }
        return cleaned;
    }
    crate::tr!("Empty query")
}

pub(crate) fn apply_editor_scheme(view: &sourceview5::View) {
    let scheme_name = if adw::StyleManager::default().is_dark() {
        "Adwaita-dark"
    } else {
        "Adwaita"
    };
    if let Some(scheme) = sourceview5::StyleSchemeManager::default().scheme(scheme_name)
        && let Ok(buffer) = view.buffer().downcast::<sourceview5::Buffer>()
    {
        buffer.set_style_scheme(Some(&scheme));
    }
}

/// Scopes the font-size CSS to source views carrying this class, rather
/// than every `textview` on the display -- which previously included the
/// EXPLAIN dialog and the JSON popover's text views too.
const EDITOR_FONT_CSS_CLASS: &str = "tp-sql-editor-font";

fn safe_font_family(family: &str) -> Option<&str> {
    let family = family.trim();
    let allowed = |c: char| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.');
    (!family.is_empty() && family.len() <= 64 && family.chars().all(allowed)).then_some(family)
}

fn editor_font_css(font_size: u32, family: &str) -> String {
    let family = safe_font_family(family)
        .map(|family| format!(" font-family: \"{family}\", monospace;"))
        .unwrap_or_default();
    format!(".{EDITOR_FONT_CSS_CLASS}, .{EDITOR_FONT_CSS_CLASS} text {{ font-size: {font_size}pt;{family} }}")
}

pub(crate) fn apply_editor_font(view: &sourceview5::View, font_size: u32, family: &str) {
    view.add_css_class(EDITOR_FONT_CSS_CLASS);
    thread_local! {
        static EDITOR_FONT_PROVIDER: std::cell::RefCell<Option<gtk::CssProvider>> =
            const { std::cell::RefCell::new(None) };
    }
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    EDITOR_FONT_PROVIDER.with(|cell| {
        if let Some(prev) = cell.borrow_mut().take() {
            gtk::style_context_remove_provider_for_display(&display, &prev);
        }
        let css = editor_font_css(font_size, family);
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&css);
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        *cell.borrow_mut() = Some(provider);
    });
}

#[cfg(test)]
mod font_tests {
    use super::*;

    #[test]
    fn an_empty_family_keeps_the_system_monospace_font() {
        assert_eq!(
            editor_font_css(12, "  "),
            ".tp-sql-editor-font, .tp-sql-editor-font text { font-size: 12pt; }"
        );
    }

    #[test]
    fn a_plain_family_name_is_quoted_with_a_monospace_fallback() {
        assert!(editor_font_css(14, "Iosevka Term").contains("font-family: \"Iosevka Term\", monospace;"));
    }

    #[test]
    fn a_family_that_could_break_out_of_the_rule_is_ignored() {
        for hostile in ["x\"; } * { color: red", "a{b}", "a;b", "\\9"] {
            assert!(!editor_font_css(12, hostile).contains("font-family"), "{hostile}");
        }
    }
}
