use gtk4 as gtk;

use super::spacing::{MD, SM};

pub(crate) fn read_only_monospace(wrap: gtk::WrapMode, vertical_margin: i32) -> gtk::TextView {
    gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(wrap)
        .top_margin(vertical_margin)
        .bottom_margin(vertical_margin)
        .left_margin(MD)
        .right_margin(MD)
        .build()
}

pub(crate) fn compact(wrap: gtk::WrapMode) -> gtk::TextView {
    read_only_monospace(wrap, SM)
}

pub(crate) fn roomy(wrap: gtk::WrapMode) -> gtk::TextView {
    read_only_monospace(wrap, MD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_shared_output_view_is_read_only_monospace_without_a_cursor() {
        gtk::init().unwrap();
        let view = compact(gtk::WrapMode::None);
        assert!(!view.is_editable());
        assert!(!view.is_cursor_visible());
        assert!(view.is_monospace());
        assert_eq!(view.top_margin(), 8);
        assert_eq!(roomy(gtk::WrapMode::WordChar).top_margin(), 12);
        assert_eq!(view.left_margin(), 12);
    }
}
