use relm4::adw::prelude::*;
use relm4::{adw, gtk};

use bookie_app_services::tab_mark::TabMark;

const DIRTY_PREFIX: &str = "• ";

fn mark_quark() -> gtk::glib::Quark {
    static QUARK: std::sync::OnceLock<gtk::glib::Quark> = std::sync::OnceLock::new();
    *QUARK.get_or_init(|| gtk::glib::Quark::from_str("tp-workspace-tab-mark"))
}

fn automatic_title_quark() -> gtk::glib::Quark {
    static QUARK: std::sync::OnceLock<gtk::glib::Quark> = std::sync::OnceLock::new();
    *QUARK.get_or_init(|| gtk::glib::Quark::from_str("tp-workspace-tab-automatic-title"))
}

pub(super) fn read_tab_mark(page: &adw::TabPage) -> TabMark {
    unsafe {
        page.qdata::<TabMark>(mark_quark())
            .map(|mark| mark.as_ref().clone())
            .unwrap_or_default()
    }
}

fn read_automatic_title(page: &adw::TabPage) -> Option<String> {
    unsafe {
        page.qdata::<String>(automatic_title_quark())
            .map(|title| title.as_ref().clone())
    }
}

fn displayed_title(mark: &TabMark, automatic: &str) -> String {
    match (mark.name(), automatic.strip_prefix(DIRTY_PREFIX)) {
        (None, _) => automatic.to_owned(),
        (Some(name), Some(_)) => format!("{DIRTY_PREFIX}{name}"),
        (Some(name), None) => name.to_owned(),
    }
}

pub(super) fn set_tab_title(page: &adw::TabPage, automatic: &str) {
    unsafe {
        page.set_qdata(automatic_title_quark(), automatic.to_owned());
    }
    page.set_title(&displayed_title(&read_tab_mark(page), automatic));
}

pub(super) fn apply_tab_mark(page: &adw::TabPage, mark: TabMark) {
    if let Some(automatic) = read_automatic_title(page) {
        page.set_title(&displayed_title(&mark, &automatic));
    }
    unsafe {
        page.set_qdata(mark_quark(), mark);
    }
}

pub(super) fn present_rename_dialog(
    parent: &impl IsA<gtk::Widget>,
    page: &adw::TabPage,
    on_renamed: impl Fn(&adw::TabPage) + 'static,
) {
    let entry = gtk::Entry::builder()
        .activates_default(true)
        .text(read_tab_mark(page).name().unwrap_or(""))
        .placeholder_text(read_automatic_title(page).unwrap_or_default())
        .build();
    let dialog = adw::AlertDialog::new(
        Some(&bookie_app_services::tr!("Rename tab")),
        Some(&bookie_app_services::tr!(
            "Leave the name empty to use the automatic title."
        )),
    );
    dialog.set_extra_child(Some(&entry));
    dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
    dialog.add_response("rename", &bookie_app_services::tr!("Rename"));
    dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("rename"));
    dialog.set_close_response("cancel");
    let page = page.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "rename" {
            return;
        }
        let renamed = read_tab_mark(&page).with_name(entry.text().as_str());
        apply_tab_mark(&page, renamed);
        on_renamed(&page);
    });
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_name_replaces_the_automatic_title_and_keeps_the_unsaved_marker() {
        let named = TabMark::default().with_name("Orders audit");
        assert_eq!(displayed_title(&named, "SELECT 1"), "Orders audit");
        assert_eq!(displayed_title(&named, "• SELECT 1"), "• Orders audit");
        assert_eq!(displayed_title(&TabMark::default(), "• SELECT 1"), "• SELECT 1");
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn renaming_survives_later_automatic_titles_and_an_empty_name_restores_them() {
        relm4::adw::init().unwrap();
        let view = adw::TabView::new();
        let page = view.append(&gtk::Box::new(gtk::Orientation::Vertical, 0));
        set_tab_title(&page, "SELECT 1");
        assert_eq!(page.title().as_str(), "SELECT 1");

        apply_tab_mark(&page, read_tab_mark(&page).with_name("Orders audit"));
        assert_eq!(page.title().as_str(), "Orders audit");
        set_tab_title(&page, "SELECT 2");
        assert_eq!(page.title().as_str(), "Orders audit");
        set_tab_title(&page, "• SELECT 2");
        assert_eq!(page.title().as_str(), "• Orders audit");

        apply_tab_mark(&page, read_tab_mark(&page).with_name("  "));
        assert_eq!(page.title().as_str(), "• SELECT 2");
    }
}
