use libadwaita as adw;
use libadwaita::prelude::*;

pub(crate) fn new(title: &str, width: i32, height: i32) -> (adw::Dialog, adw::ToolbarView) {
    build(title, &adw::HeaderBar::new(), width, Some(height))
}

pub(crate) fn form(title: &str, width: i32) -> (adw::Dialog, adw::ToolbarView) {
    build(title, &adw::HeaderBar::new(), width, None)
}

pub(crate) fn with_header(
    title: &str,
    header: &adw::HeaderBar,
    width: i32,
    height: Option<i32>,
) -> (adw::Dialog, adw::ToolbarView) {
    build(title, header, width, height)
}

fn build(title: &str, header: &adw::HeaderBar, width: i32, height: Option<i32>) -> (adw::Dialog, adw::ToolbarView) {
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(header);
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(width)
        .child(&toolbar)
        .build();
    match height {
        Some(height) => dialog.set_content_height(height),
        None => dialog.set_follows_content_size(true),
    }
    (dialog, toolbar)
}
