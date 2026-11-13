use std::cell::RefCell;

use bookie_app_services::preferences::TypographyOverride;

thread_local! {
    static PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };
}

pub(crate) fn apply(preferences: &crate::services::preferences::Preferences) {
    let Some(display) = gtk4::gdk::Display::default() else {
        return;
    };
    PROVIDER.with(|cell| {
        if let Some(provider) = cell.borrow_mut().take() {
            gtk4::style_context_remove_provider_for_display(&display, &provider);
        }
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(&format!(
            "{}{}",
            font_css("window", &preferences.ui_typography),
            font_css(".bookie-output-font", &preferences.output_typography),
        ));
        gtk4::style_context_add_provider_for_display(&display, &provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
        *cell.borrow_mut() = Some(provider);
    });
}

pub(crate) fn safe_font_family(family: &str) -> Option<&str> {
    let family = family.trim();
    let allowed = |character: char| character.is_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.');
    (!family.is_empty() && family.len() <= 64 && family.chars().all(allowed)).then_some(family)
}

fn font_css(selector: &str, typography: &TypographyOverride) -> String {
    let mut declarations = String::new();
    if let Some(family) = safe_font_family(&typography.font_family) {
        declarations.push_str(&format!("font-family: \"{family}\";"));
    }
    if typography.font_size > 0 {
        declarations.push_str(&format!("font-size: {}pt;", typography.font_size));
    }
    if typography.font_weight > 0 {
        declarations.push_str(&format!("font-weight: {};", typography.font_weight));
    }
    if declarations.is_empty() {
        return String::new();
    }
    format!("{selector} {{ {declarations} }}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_css_omits_system_defaults_and_rejects_css_syntax() {
        assert_eq!(font_css("window", &TypographyOverride::default()), "");
        let typography = TypographyOverride {
            font_family: "Inter; color: red".into(),
            font_size: 20,
            font_weight: 600,
        };
        assert_eq!(
            font_css("window", &typography),
            "window { font-size: 20pt;font-weight: 600; }"
        );
    }
}
