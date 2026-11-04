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
    pending(window) > 0
}

fn is_last_window(window: &adw::ApplicationWindow) -> bool {
    window.application().is_none_or(|app| app.windows().len() <= 1)
}

fn pending(window: &adw::ApplicationWindow) -> usize {
    let jobs = busy_jobs::global();
    if is_last_window(window) {
        jobs.active()
    } else {
        jobs.active_owned_by(crate::ui::progress_dialog::owner_id(window))
    }
}

fn cancel_pending(window: &adw::ApplicationWindow) {
    let jobs = busy_jobs::global();
    if is_last_window(window) {
        jobs.cancel_all();
    } else {
        jobs.cancel_owned_by(crate::ui::progress_dialog::owner_id(window));
    }
}

pub(super) fn confirm(window: &adw::ApplicationWindow, confirmed: Rc<Cell<bool>>) {
    let dialog = alert();
    let window_for_response = window.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "quit" {
            cancel_pending(&window_for_response);
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
        if pending(&window) > 0 && ticks < MAX_WAIT_TICKS {
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

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn with_two_windows_only_the_one_that_owns_running_work_asks_before_closing() {
        gtk::init().unwrap();
        adw::init().unwrap();
        let application = adw::Application::builder()
            .application_id("com.bookie.linux.busy-quit-owner-test")
            .build();
        application.register(None::<&gtk::gio::Cancellable>).unwrap();
        let (owner, other) = (
            adw::ApplicationWindow::new(&application),
            adw::ApplicationWindow::new(&application),
        );
        owner.present();
        other.present();
        assert!(!needed(&owner) && !needed(&other), "nothing runs yet");
        let progress = crate::ui::progress_dialog::CancellableDialog::present(&owner, "Importing into example", &[]);
        assert!(needed(&owner), "the window that owns the job must ask");
        assert!(!needed(&other), "another window closes without asking");
        other.close();
        assert!(needed(&owner), "the last window asks for any running work");
        progress.close();
        drop(progress);
        assert!(!needed(&owner));
        owner.close();
    }
}
