use libadwaita::prelude::*;

pub(super) fn restore_on_close(
    dialog: &libadwaita::Dialog,
    view: &gtk4::ColumnView,
    on_restored: impl Fn(&gtk4::ColumnView) + 'static,
) {
    let on_restored = std::rc::Rc::new(on_restored);
    dialog.connect_closed({
        let view = view.downgrade();
        move |_| {
            let origin = view
                .upgrade()
                .and_then(|view| view.root())
                .and_then(|root| root.focus())
                .map(|focus| focus.downgrade());
            let view = view.clone();
            let on_restored = on_restored.clone();
            gtk4::glib::idle_add_local_once(move || {
                let Some(view) = view.upgrade().filter(WidgetExt::is_mapped) else {
                    return;
                };
                let current = view.root().and_then(|root| root.focus());
                let origin = origin.and_then(|focus| focus.upgrade());
                if current.as_ref() != origin.as_ref() {
                    return;
                }
                on_restored(&view);
                focus_navigation_row(&view);
            });
        }
    });
}

fn focus_navigation_row(view: &gtk4::ColumnView) {
    view.grab_focus();
    let Some(focus) = view.root().and_then(|root| root.focus()) else {
        return;
    };
    if !focus.is::<gtk4::Label>() || !focus.is_ancestor(view) {
        return;
    }
    if let Some(cell) = focus
        .parent()
        .filter(|parent| parent.accessible_role() == gtk4::AccessibleRole::GridCell && parent.is_focusable())
    {
        cell.child_focus(gtk4::DirectionType::TabBackward);
    }
}

#[cfg(test)]
#[path = "dialog_focus_tests.rs"]
mod tests;
