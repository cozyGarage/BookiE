mod dialog;
mod json_search;
mod query;
mod replace;

use std::rc::Rc;

use gtk4 as gtk;
use gtk4::prelude::*;

pub(super) use dialog::FindTarget;
use dialog::present;

pub(super) fn install(view: &gtk::ColumnView, actions: &gtk::gio::SimpleActionGroup, target: FindTarget) {
    let action = gtk::gio::SimpleAction::new("find-row", None);
    let weak_view = view.downgrade();
    let target = Rc::new(target);
    action.connect_activate(move |_, _| {
        if let Some(view) = weak_view.upgrade().filter(|view| view.is_mapped()) {
            present(&view, target.clone());
        }
    });
    actions.add_action(&action);
    let controller = gtk::ShortcutController::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    controller.add_shortcut(
        gtk::Shortcut::builder()
            .trigger(&crate::ui::shortcut::parse("<Primary><Alt>f"))
            .action(&gtk::NamedAction::new("grid.find-row"))
            .build(),
    );
    view.add_controller(controller);
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
