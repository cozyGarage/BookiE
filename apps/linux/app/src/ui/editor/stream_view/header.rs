use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

pub(in crate::ui::editor) const HEADER_NAME: &str = "tp-result-header";
const PROGRESS_MAX_CHARS: i32 = 44;

pub(in crate::ui::editor) fn build(progress: &gtk::Label) -> adw::WrapBox {
    let header = adw::WrapBox::builder()
        .child_spacing(8)
        .line_spacing(4)
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(6)
        .margin_end(6)
        .build();
    header.set_widget_name(HEADER_NAME);
    header.add_css_class("toolbar");
    progress.set_xalign(0.0);
    progress.set_wrap(true);
    progress.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    progress.set_lines(2);
    progress.set_ellipsize(gtk::pango::EllipsizeMode::End);
    progress.set_max_width_chars(PROGRESS_MAX_CHARS);
    header.append(progress);
    header
}
