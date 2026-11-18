use gtk4 as gtk;
use libadwaita::{self as adw, prelude::*};

pub(super) use bookie_app_services::edit_preview::preview_text;

fn preview_view(text: &str) -> gtk::TextView {
    let view = crate::ui::output_text_view::roomy(gtk::WrapMode::WordChar);
    view.buffer().set_text(text);
    view
}

pub(super) fn present(parent: &impl IsA<gtk::Widget>, text: &str) {
    let view = preview_view(text);
    let scrolled = gtk::ScrolledWindow::builder().vexpand(true).child(&view).build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&scrolled));
    let dialog = adw::Dialog::builder()
        .title(bookie_app_services::tr!("Pending SQL"))
        .content_width(640)
        .content_height(420)
        .child(&toolbar)
        .build();
    crate::ui::leak_probe::present_tracked("pending_sql", &dialog, parent);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_preview_is_read_only_and_shows_no_blinking_caret() {
        gtk::init().unwrap();
        let view = preview_view("UPDATE t SET a = 1;");
        assert!(!view.is_editable());
        assert!(!view.is_cursor_visible(), "a read-only view must not blink a caret");
    }
}
