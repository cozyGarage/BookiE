use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use bookie_app_services::busy_jobs;
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk::glib;

const WAIT_TICK: Duration = Duration::from_millis(100);
const MAX_WAIT_TICKS: u32 = 300;

pub(super) fn needed(window: &adw::ApplicationWindow) -> bool {
    let last_window = window.application().is_none_or(|app| app.windows().len() <= 1);
    last_window && busy_jobs::global().active() > 0
}

pub(super) fn confirm(window: &adw::ApplicationWindow, confirmed: Rc<Cell<bool>>) {
    let dialog = alert();
    let window_for_response = window.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "quit" {
            busy_jobs::global().cancel_all();
            close_when_idle(&window_for_response, confirmed.clone());
        }
    });
    dialog.present(Some(window));
}

fn alert() -> adw::AlertDialog {
    let dialog = adw::AlertDialog::new(
        Some(&crate::tr!("Work is still running")),
        Some(&crate::tr!(
            "An import, export or backup has not finished. Quitting cancels it and waits for it to stop."
        )),
    );
    dialog.add_response("keep", &crate::tr!("Keep Working"));
    dialog.add_response("quit", &crate::tr!("Cancel Work and Quit"));
    dialog.set_response_appearance("quit", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("keep");
    dialog
}

fn close_when_idle(window: &adw::ApplicationWindow, confirmed: Rc<Cell<bool>>) {
    let window = window.clone();
    let mut ticks = 0;
    glib::timeout_add_local(WAIT_TICK, move || {
        ticks += 1;
        if busy_jobs::global().active() > 0 && ticks < MAX_WAIT_TICKS {
            return glib::ControlFlow::Continue;
        }
        confirmed.set(true);
        window.close();
        glib::ControlFlow::Break
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::gtk;

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn the_confirm_defaults_to_keeping_the_work_and_marks_quitting_destructive() {
        gtk::init().unwrap();
        adw::init().unwrap();
        let dialog = alert();
        assert_eq!(dialog.default_response().as_deref(), Some("keep"));
        assert_eq!(dialog.close_response(), "keep");
        assert_eq!(dialog.response_appearance("quit"), adw::ResponseAppearance::Destructive);
        assert!(dialog.has_response("keep") && dialog.has_response("quit"));
    }
}
