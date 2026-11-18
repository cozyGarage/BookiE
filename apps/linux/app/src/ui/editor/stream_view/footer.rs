use crate::ui::spacing;
use relm4::gtk;
use relm4::gtk::prelude::*;

pub(in crate::ui::editor) const FOOTER_NAME: &str = "tp-result-footer";
const PROGRESS_MAX_CHARS: i32 = 60;

pub(in crate::ui::editor) fn build(progress: &gtk::Label, pager: &gtk::Box) -> gtk::Box {
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, spacing::MD);
    footer.set_widget_name(FOOTER_NAME);
    footer.add_css_class("toolbar");
    progress.set_xalign(0.0);
    progress.set_hexpand(true);
    progress.set_wrap(true);
    progress.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    progress.set_lines(2);
    progress.set_ellipsize(gtk::pango::EllipsizeMode::End);
    progress.set_max_width_chars(PROGRESS_MAX_CHARS);
    pager.set_halign(gtk::Align::End);
    footer.append(progress);
    footer.append(pager);
    footer
}
