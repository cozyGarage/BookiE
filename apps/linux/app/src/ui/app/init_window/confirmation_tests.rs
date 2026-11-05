use std::{cell::Cell, rc::Rc, time::Duration};

use relm4::adw::prelude::*;

use super::connect_close_confirmation_response;

#[test]
#[ignore = "requires an isolated GTK display"]
fn force_dismissing_window_close_confirmation_keeps_the_window_open() {
    relm4::adw::init().unwrap();
    relm4::gtk::gio::resources_register_include!("bookie.gresource").unwrap();
    let application = relm4::main_application();
    application.register(None::<&relm4::gtk::gio::Cancellable>).unwrap();
    let window = relm4::adw::ApplicationWindow::new(&application);
    window.present();
    spin_until(|| window.is_mapped());

    let dialog = relm4::adw::AlertDialog::new(None, None);
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("discard", "Discard");
    dialog.set_close_response("cancel");
    let discard_called = Rc::new(Cell::new(false));
    let discard_called_for_response = discard_called.clone();
    connect_close_confirmation_response(&dialog, move |response| {
        if response == "discard" {
            discard_called_for_response.set(true);
        }
    });
    let closed = Rc::new(Cell::new(false));
    let closed_for_signal = closed.clone();
    dialog.connect_closed(move |_| closed_for_signal.set(true));
    dialog.present(Some(&window));
    spin_until(|| dialog.is_mapped());

    dialog.force_close();
    spin_until(|| closed.get());

    assert!(!discard_called.get());
    assert!(window.is_mapped());
}

fn spin_until(mut condition: impl FnMut() -> bool) {
    let context = relm4::gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !condition() && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(condition());
}
