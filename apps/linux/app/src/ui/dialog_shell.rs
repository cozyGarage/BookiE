use libadwaita as adw;

pub(crate) fn new(title: &str, width: i32, height: i32) -> (adw::Dialog, adw::ToolbarView) {
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(width)
        .content_height(height)
        .child(&toolbar)
        .build();
    (dialog, toolbar)
}
