use relm4::gtk;

pub(super) fn install_pending_change_css() {
    // Native Adwaita state classes wash out grid rows, so pending states use translucent accents.
    if let Some(display) = gtk::gdk::Display::default() {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/com/bookie/linux/style.css");
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}
