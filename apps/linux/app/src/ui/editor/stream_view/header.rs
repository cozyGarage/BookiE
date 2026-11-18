use crate::ui::spacing;
use relm4::adw;
use relm4::adw::prelude::*;

pub(in crate::ui::editor) const HEADER_NAME: &str = "tp-result-header";
pub(in crate::ui::editor) fn build() -> adw::WrapBox {
    let header = adw::WrapBox::builder()
        .child_spacing(spacing::SM)
        .line_spacing(spacing::XS)
        .margin_top(spacing::XS)
        .margin_bottom(spacing::XS)
        .margin_start(spacing::SM)
        .margin_end(spacing::SM)
        .build();
    header.set_widget_name(HEADER_NAME);
    header.add_css_class("toolbar");
    header
}
