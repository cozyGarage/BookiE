use relm4::adw::prelude::*;
use relm4::{adw, gtk};

use std::cell::RefCell;
use std::rc::Rc;

use relm4::gtk::{gdk, gio, glib};

use bookie_app_services::tab_mark::{TabColor, TabMark};

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
    let keywords = mark.tags().join(" ");
    page.set_keyword(&keywords);
    page.set_icon(
        mark.color()
            .map(color_dot)
            .as_ref()
            .map(|dot| dot.upcast_ref::<gio::Icon>()),
    );
    unsafe {
        page.set_qdata(mark_quark(), mark);
    }
}

fn color_rgb(color: TabColor) -> [u8; 3] {
    match color {
        TabColor::Red => [0xe0, 0x1b, 0x24],
        TabColor::Orange => [0xff, 0x78, 0x00],
        TabColor::Yellow => [0xf5, 0xc2, 0x11],
        TabColor::Green => [0x33, 0xd1, 0x7a],
        TabColor::Teal => [0x2e, 0xc4, 0xb6],
        TabColor::Blue => [0x35, 0x84, 0xe4],
        TabColor::Purple => [0x91, 0x41, 0xac],
        TabColor::Pink => [0xe8, 0x6f, 0xa3],
    }
}

fn color_label(color: TabColor) -> String {
    match color {
        TabColor::Red => bookie_app_services::tr!("Red"),
        TabColor::Orange => bookie_app_services::tr!("Orange"),
        TabColor::Yellow => bookie_app_services::tr!("Yellow"),
        TabColor::Green => bookie_app_services::tr!("Green"),
        TabColor::Teal => bookie_app_services::tr!("Teal"),
        TabColor::Blue => bookie_app_services::tr!("Blue"),
        TabColor::Purple => bookie_app_services::tr!("Purple"),
        TabColor::Pink => bookie_app_services::tr!("Pink"),
    }
}

const DOT_SIZE: i32 = 32;

fn dot_pixels(rgb: [u8; 3]) -> Vec<u8> {
    let centre = (DOT_SIZE as f32 - 1.0) / 2.0;
    let radius = DOT_SIZE as f32 * 0.34;
    let mut pixels = Vec::with_capacity((DOT_SIZE * DOT_SIZE * 4) as usize);
    for y in 0..DOT_SIZE {
        for x in 0..DOT_SIZE {
            let distance = ((x as f32 - centre).powi(2) + (y as f32 - centre).powi(2)).sqrt();
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            for channel in rgb {
                pixels.push((f32::from(channel) * coverage).round() as u8);
            }
            pixels.push((255.0 * coverage).round() as u8);
        }
    }
    pixels
}

fn color_dot(color: TabColor) -> gdk::Texture {
    gdk::MemoryTexture::new(
        DOT_SIZE,
        DOT_SIZE,
        gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &glib::Bytes::from_owned(dot_pixels(color_rgb(color))),
        (DOT_SIZE * 4) as usize,
    )
    .upcast()
}

pub(super) fn mark_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(
        Some(&bookie_app_services::tr!("Name and tags\u{2026}")),
        Some("tab.rename"),
    );
    let colours = gio::Menu::new();
    for color in TabColor::ALL {
        colours.append(Some(&color_label(color)), Some(&format!("tab.color::{}", color.id())));
    }
    colours.append(Some(&bookie_app_services::tr!("No colour")), Some("tab.color::none"));
    menu.append_submenu(Some(&bookie_app_services::tr!("Colour mark")), &colours);
    menu
}

pub(super) fn add_mark_actions(
    group: &gio::SimpleActionGroup,
    menu_target: Rc<RefCell<Option<adw::TabPage>>>,
    view: &adw::TabView,
    on_changed: impl Fn() + Clone + 'static,
) {
    let rename = gio::SimpleAction::new("rename", None);
    let (target, parent, changed) = (menu_target.clone(), view.clone(), on_changed.clone());
    rename.connect_activate(move |_, _| {
        let page = target.borrow().clone();
        if let Some(page) = page {
            let changed = changed.clone();
            present_rename_dialog(&parent, &page, move |_| changed());
        }
    });
    group.add_action(&rename);

    let colour = gio::SimpleAction::new("color", Some(glib::VariantTy::STRING));
    colour.connect_activate(move |_, value| {
        let page = menu_target.borrow().clone();
        let id = value.and_then(|value| value.str().map(str::to_owned));
        if let (Some(page), Some(id)) = (page, id) {
            let color = TabColor::ALL.into_iter().find(|color| color.id() == id);
            apply_tab_mark(&page, read_tab_mark(&page).with_color(color));
            on_changed();
        }
    });
    group.add_action(&colour);
}

pub(super) fn tab_subtitle(base: &str, mark: &TabMark) -> String {
    if mark.tags().is_empty() {
        return base.to_owned();
    }
    format!("{base} · {}", mark.tags().join(", "))
}

fn split_tags(text: &str) -> impl Iterator<Item = &str> {
    text.split([',', ';'])
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
    let tags_entry = gtk::Entry::builder()
        .activates_default(true)
        .text(read_tab_mark(page).tags().join(", "))
        .placeholder_text(bookie_app_services::tr!("Tags, separated by commas"))
        .build();
    let fields = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    fields.append(&entry);
    fields.append(&tags_entry);
    let dialog = adw::AlertDialog::new(
        Some(&bookie_app_services::tr!("Name and tags")),
        Some(&bookie_app_services::tr!(
            "Leave the name empty to use the automatic title."
        )),
    );
    dialog.set_extra_child(Some(&fields));
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
        let renamed = read_tab_mark(&page)
            .with_name(entry.text().as_str())
            .with_tags(split_tags(tags_entry.text().as_str()));
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
    fn tags_show_in_the_open_tab_subtitle_and_open_quickly_finds_them() {
        let plain = TabMark::default();
        let tagged = TabMark::default().with_tags(["audit", "q4"]);
        assert_eq!(tab_subtitle("Open tab", &plain), "Open tab");
        assert_eq!(tab_subtitle("Open tab", &tagged), "Open tab · audit, q4");
        assert_eq!(split_tags("audit, q4;  ").collect::<Vec<_>>(), ["audit", " q4", "  "]);
        let item = bookie_app_services::quick_switcher::QuickItem {
            target: bookie_app_services::quick_switcher::QuickTarget::Tab(uuid::Uuid::new_v4()),
            title: "SELECT 1".into(),
            subtitle: tab_subtitle("Open tab", &tagged),
        };
        assert_eq!(
            bookie_app_services::quick_switcher::filter(std::slice::from_ref(&item), "q4").len(),
            1
        );
        assert!(bookie_app_services::quick_switcher::filter(&[item], "zzz").is_empty());
    }

    #[test]
    fn the_colour_dot_is_opaque_in_the_middle_and_clear_at_the_corners() {
        let pixels = dot_pixels([0x33, 0x84, 0xe4]);
        let at = |x: i32, y: i32| {
            let start = ((y * DOT_SIZE + x) * 4) as usize;
            &pixels[start..start + 4]
        };
        assert_eq!(at(DOT_SIZE / 2, DOT_SIZE / 2), [0x33, 0x84, 0xe4, 255]);
        assert_eq!(at(0, 0)[3], 0);
        assert_eq!(pixels.len(), (DOT_SIZE * DOT_SIZE * 4) as usize);
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn the_colour_mark_shows_as_the_tab_icon_and_clears_with_no_colour() {
        relm4::adw::init().unwrap();
        let view = adw::TabView::new();
        let page = view.append(&gtk::Box::new(gtk::Orientation::Vertical, 0));
        let group = gio::SimpleActionGroup::new();
        let target = Rc::new(RefCell::new(Some(page.clone())));
        let changed = Rc::new(std::cell::Cell::new(0));
        let seen = changed.clone();
        add_mark_actions(&group, target, &view, move || seen.set(seen.get() + 1));
        assert!(page.icon().is_none());

        group.activate_action("color", Some(&"teal".to_variant()));
        assert!(page.icon().is_some());
        assert_eq!(read_tab_mark(&page).color(), Some(TabColor::Teal));
        group.activate_action("color", Some(&"none".to_variant()));
        assert!(page.icon().is_none());
        assert_eq!(read_tab_mark(&page).color(), None);
        assert_eq!(changed.get(), 2);
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

        apply_tab_mark(&page, read_tab_mark(&page).with_tags(["audit", "q4"]));
        assert_eq!(page.keyword().as_deref(), Some("audit q4"));
        apply_tab_mark(&page, read_tab_mark(&page).with_tags([]));
        assert!(page.keyword().is_none_or(|keyword| keyword.is_empty()));
    }
}
