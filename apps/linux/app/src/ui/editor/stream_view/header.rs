use relm4::adw;
use relm4::adw::prelude::*;

pub(in crate::ui::editor) const HEADER_NAME: &str = "tp-result-header";
pub(in crate::ui::editor) fn build() -> adw::WrapBox {
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
    header
}
