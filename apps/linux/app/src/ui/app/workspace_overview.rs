use relm4::adw::prelude::*;
use relm4::{adw, gtk};

pub(super) fn build_workspace_tab_overview(view: &adw::TabView, child: &impl IsA<gtk::Widget>) -> adw::TabOverview {
    adw::TabOverview::builder()
        .view(view)
        .enable_new_tab(true)
        .enable_search(true)
        .show_end_title_buttons(false)
        .child(child)
        .build()
}

pub(super) fn watch_overview_tags(view: &adw::TabView, overview: &adw::TabOverview) {
    let view_for_open = view.downgrade();
    overview.connect_open_notify(move |overview| {
        if let Some(view) = view_for_open.upgrade() {
            super::tab_title::sync_overview_tag_titles(&view, overview.is_open());
        }
    });
    let overview_for_pages = overview.downgrade();
    view.connect_page_attached(move |_, page, _| {
        if let Some(overview) = overview_for_pages.upgrade() {
            super::tab_title::set_overview_tags_visible(page, overview.is_open());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::tab_title;
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_tab_overview_hides_duplicate_window_close_controls() {
        gtk::init().unwrap();
        let view = adw::TabView::new();
        let child = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let overview = build_workspace_tab_overview(&view, &child);

        assert!(!overview.shows_end_title_buttons());
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn overview_titles_show_current_tags_and_restore_titles_when_closed() {
        relm4::adw::init().unwrap();
        let view = adw::TabView::new();
        let child = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let overview = build_workspace_tab_overview(&view, &child);
        watch_overview_tags(&view, &overview);

        let first = view.append(&gtk::Box::new(gtk::Orientation::Vertical, 0));
        tab_title::set_tab_title(&first, "SELECT 1");
        tab_title::apply_tab_mark(
            &first,
            tab_title::read_tab_mark(&first)
                .with_name("Orders")
                .with_tags(["audit", "q4"]),
        );
        overview.set_open(true);
        assert_eq!(first.title().as_str(), "Orders · audit, q4");

        let added = view.append(&gtk::Box::new(gtk::Orientation::Vertical, 0));
        tab_title::set_tab_title(&added, "SELECT 2");
        tab_title::apply_tab_mark(&added, tab_title::read_tab_mark(&added).with_tags(["billing"]));
        assert_eq!(added.title().as_str(), "SELECT 2 · billing");

        tab_title::apply_tab_mark(&first, tab_title::read_tab_mark(&first).with_tags(["finance"]));
        assert_eq!(first.title().as_str(), "Orders · finance");
        overview.set_open(false);
        assert_eq!(first.title().as_str(), "Orders");
        assert_eq!(added.title().as_str(), "SELECT 2");
    }
}
