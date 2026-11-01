use gtk4::{self as gtk, glib, prelude::*};

use super::{GridMenus, cell_menu_options, is_cell_menu_widget, prepare_cell_menu};

pub(super) fn install(view: &gtk::ColumnView, menus: &GridMenus) {
    let weak_view = view.downgrade();
    let menus = menus.clone();
    let shortcut = gtk::Shortcut::builder()
        .trigger(&menus.cell_menu_trigger)
        .action(&gtk::CallbackAction::new(move |_, _| {
            let Some(view) = weak_view.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let Some(widget) = focused_cell(&view, &menus) else {
                return glib::Propagation::Proceed;
            };
            let Some(options) = cell_menu_options(&menus, &widget) else {
                return glib::Propagation::Proceed;
            };
            let popover = prepare_cell_menu(&menus, &view, &widget, &options);
            popover.set_pointing_to(
                widget
                    .compute_bounds(&view)
                    .map(|bounds| {
                        gtk::gdk::Rectangle::new(
                            bounds.x() as i32,
                            bounds.y() as i32,
                            bounds.width() as i32,
                            bounds.height() as i32,
                        )
                    })
                    .as_ref(),
            );
            popover.popup();
            glib::Propagation::Stop
        }))
        .build();
    let controller = gtk::ShortcutController::new();
    controller.set_scope(gtk::ShortcutScope::Managed);
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    controller.add_shortcut(shortcut);
    view.add_controller(controller);
}

fn focused_cell(view: &gtk::ColumnView, menus: &GridMenus) -> Option<gtk::Widget> {
    let window = view.root()?.downcast::<gtk::Window>().ok()?;
    let mut widget = gtk::prelude::GtkWindowExt::focus(&window)?;
    if !widget.is_ancestor(view) {
        return None;
    }
    loop {
        let cell = descendant_cell(&widget, menus);
        if let Some(cell) = cell {
            if cell
                .downcast_ref::<crate::ui::cell_editor::CellEditor>()
                .is_some_and(|editor| editor.is_editing())
            {
                return None;
            }
            return Some(cell);
        }
        widget = widget.parent()?;
        if widget == *view {
            return None;
        }
    }
}

fn descendant_cell(widget: &gtk::Widget, menus: &GridMenus) -> Option<gtk::Widget> {
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(cell) = descendant_cell(&widget, menus) {
            return Some(cell);
        }
        child = widget.next_sibling();
    }
    is_cell_menu_widget(menus, widget).then(|| widget.clone())
}
