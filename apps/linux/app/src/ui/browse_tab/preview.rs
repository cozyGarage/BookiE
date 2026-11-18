use gtk4 as gtk;
use libadwaita::prelude::*;

pub(super) use bookie_app_services::edit_preview::preview_text;

fn preview_view(text: &str) -> gtk::TextView {
    let view = crate::ui::output_text_view::roomy(gtk::WrapMode::WordChar);
    view.buffer().set_text(text);
    view
}

pub(super) fn present(parent: &impl IsA<gtk::Widget>, text: &str) {
    let view = preview_view(text);
    let scrolled = gtk::ScrolledWindow::builder().vexpand(true).child(&view).build();
    let (dialog, toolbar) = crate::ui::dialog_shell::new(&bookie_app_services::tr!("Pending SQL"), 640, 420);
    toolbar.set_content(Some(&scrolled));
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
