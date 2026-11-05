use relm4::adw;
use relm4::gtk::glib;

pub(crate) fn toast(message: &str) -> adw::Toast {
    let toast = adw::Toast::new(message);
    toast.set_use_markup(false);
    toast
}

pub(crate) fn escape_markup(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

#[cfg(test)]
#[path = "plain_text_tests.rs"]
mod tests;
