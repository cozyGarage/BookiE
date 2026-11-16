use relm4::adw::prelude::*;
use relm4::gtk;

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
    bookie_app_services::tr!("Empty query")
}

pub(crate) fn apply_editor_scheme(view: &sourceview5::View) {
    crate::ui::theme::style_editor(view);
}

/// Scopes the font-size CSS to source views carrying this class, rather
/// than every `textview` on the display -- which previously included the
/// EXPLAIN dialog and the JSON popover's text views too.
const EDITOR_FONT_CSS_CLASS: &str = "tp-sql-editor-font";
const EDITOR_LINE_HEIGHT: &str = "1.2";

fn editor_font_css(font_size: u32, family: &str, weight: u32) -> String {
    let family = crate::ui::typography::safe_font_family(family)
        .map(|family| format!(" font-family: \"{family}\", monospace;"))
        .unwrap_or_default();
    let size = if font_size > 0 {
        format!(" font-size: {font_size}pt;")
    } else {
        String::new()
    };
    let weight = if weight > 0 {
        format!(" font-weight: {weight};")
    } else {
        String::new()
    };
    format!("textview.{EDITOR_FONT_CSS_CLASS} {{{size}{family}{weight} line-height: {EDITOR_LINE_HEIGHT}; }}")
}

fn apply_vim_mode(view: &sourceview5::View, enabled: bool) {
    if !enabled {
        return;
    }
    let context = sourceview5::VimIMContext::new();
    context.set_client_widget(Some(view));
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.set_im_context(Some(&context));
    view.add_controller(keys);
    view.connect_destroy(move |_| context.set_client_widget(gtk::Widget::NONE));
}

pub(super) fn release_vim_context(view: &sourceview5::View) {
    for controller in view.observe_controllers().iter::<gtk::glib::Object>().flatten() {
        let Ok(keys) = controller.downcast::<gtk::EventControllerKey>() else {
            continue;
        };
        let Some(context) = keys.im_context().and_downcast::<sourceview5::VimIMContext>() else {
            continue;
        };
        keys.set_im_context(None::<&gtk::IMContext>);
        context.set_client_widget(gtk::Widget::NONE);
    }
}

pub(crate) fn apply_editor_preferences(
    view: &sourceview5::View,
    preferences: &crate::services::preferences::Preferences,
) {
    apply_editor_font(
        view,
        preferences.editor_font_size,
        &preferences.editor_font_family,
        preferences.editor_font_weight,
    );
    apply_vim_mode(view, preferences.editor_vim_mode);
}

pub(super) fn apply_editor_font_preferences(preferences: &crate::services::preferences::Preferences) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    apply_editor_font_provider(
        &display,
        preferences.editor_font_size,
        &preferences.editor_font_family,
        preferences.editor_font_weight,
    );
}

fn apply_editor_font(view: &sourceview5::View, font_size: u32, family: &str, weight: u32) {
    view.add_css_class(EDITOR_FONT_CSS_CLASS);
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    apply_editor_font_provider(&display, font_size, family, weight);
}

fn apply_editor_font_provider(display: &gtk::gdk::Display, font_size: u32, family: &str, weight: u32) {
    thread_local! {
        static EDITOR_FONT_PROVIDER: std::cell::RefCell<Option<gtk::CssProvider>> =
            const { std::cell::RefCell::new(None) };
    }
    EDITOR_FONT_PROVIDER.with(|cell| {
        if let Some(prev) = cell.borrow_mut().take() {
            gtk::style_context_remove_provider_for_display(display, &prev);
        }
        let css = editor_font_css(font_size, family, weight);
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&css);
        gtk::style_context_add_provider_for_display(display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        *cell.borrow_mut() = Some(provider);
    });
}

#[cfg(test)]
mod font_tests {
    use super::*;

    #[test]
    fn an_empty_family_keeps_the_system_monospace_font() {
        assert_eq!(
            editor_font_css(12, "  ", 0),
            "textview.tp-sql-editor-font { font-size: 12pt; line-height: 1.2; }"
        );
    }

    #[test]
    fn a_plain_family_name_is_quoted_with_a_monospace_fallback() {
        assert!(editor_font_css(14, "Iosevka Term", 0).contains("font-family: \"Iosevka Term\", monospace;"));
    }

    #[test]
    fn editor_font_css_targets_the_textview_node_with_unitless_line_height() {
        let css = editor_font_css(16, "JetBrains Mono", 0);
        assert!(css.starts_with("textview.tp-sql-editor-font {"), "{css}");
        assert!(!css.contains(".tp-sql-editor-font text"), "{css}");
        assert!(css.contains("font-size: 16pt;"));
        assert!(css.contains(&format!("line-height: {EDITOR_LINE_HEIGHT};")));
        assert!(!css.contains("line-height: 1.2px"));
        assert!(!css.contains("pixels-above"));
    }

    #[test]
    fn a_family_that_could_break_out_of_the_rule_is_ignored() {
        for hostile in ["x\"; } * { color: red", "a{b}", "a;b", "\\9"] {
            assert!(!editor_font_css(12, hostile, 0).contains("font-family"), "{hostile}");
        }
    }
}

#[cfg(test)]
mod vim_tests {
    use super::*;

    fn controllers(view: &sourceview5::View) -> u32 {
        view.observe_controllers().n_items()
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn vim_keys_are_attached_only_when_enabled() {
        gtk::init().unwrap();
        let plain = sourceview5::View::new();
        let before = controllers(&plain);
        apply_vim_mode(&plain, false);
        assert_eq!(controllers(&plain), before);

        let vim = sourceview5::View::new();
        let before = controllers(&vim);
        apply_vim_mode(&vim, true);
        assert!(controllers(&vim) > before);
        std::mem::forget(vim);
    }
}
