use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::OnceLock;

use relm4::gtk;
use relm4::gtk::prelude::*;
use relm4::gtk::{gio, glib};

type Tracked = Vec<(&'static str, glib::WeakRef<glib::Object>)>;

thread_local! {
    static TRACKED: RefCell<Tracked> = const { RefCell::new(Vec::new()) };
}

fn report_path() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var_os("TABLEPRO_GTK_LEAK_REPORT").map(PathBuf::from))
        .as_ref()
}

pub(crate) fn track(name: &'static str, object: &impl IsA<glib::Object>) {
    if report_path().is_some() {
        TRACKED.with(|tracked| tracked.borrow_mut().push((name, object.upcast_ref().downgrade())));
    }
}

fn report() -> String {
    let mut lines = vec![format!("toplevels={}", gtk::Window::list_toplevels().len())];
    TRACKED.with(|tracked| {
        let mut tracked = tracked.borrow_mut();
        tracked.retain(|(_, weak)| weak.upgrade().is_some());
        let mut names: Vec<_> = tracked.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        for name in names {
            lines.push(format!("{name}={}", tracked.iter().filter(|(n, _)| *n == name).count()));
        }
    });
    lines.join("\n")
}

pub(crate) fn install(group: &gio::SimpleActionGroup) {
    let Some(path) = report_path() else { return };
    let action = gio::SimpleAction::new("report-leaks", None);
    action.connect_activate(move |_, _| {
        if let Err(error) = std::fs::write(path, report()) {
            tracing::warn!(%error, "could not write the leak report");
        }
    });
    group.add_action(&action);
}
