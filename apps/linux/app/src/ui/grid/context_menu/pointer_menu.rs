use gtk4::{self as gtk, prelude::*};

use super::{GridMenus, cell_menu_options, is_cell_menu_widget, prepare_cell_menu};

pub(super) fn install(view: &gtk::ColumnView, menus: &GridMenus, empty: gtk::PopoverMenu) {
    let weak_view = view.downgrade();
    let menus = menus.clone();
    let gesture = gtk::GestureClick::builder()
        .button(gtk::gdk::BUTTON_SECONDARY)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(view) = weak_view.upgrade() else { return };
        if let Some(popover) = popover_at(&view, &menus, &empty, x, y) {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            crate::ui::context_menu::popup_at(&popover, Some(gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        }
    });
    view.add_controller(gesture);
}

pub(super) fn popover_at(
    view: &gtk::ColumnView,
    menus: &GridMenus,
    empty: &gtk::PopoverMenu,
    x: f64,
    y: f64,
) -> Option<gtk::PopoverMenu> {
    let mut widget = view.pick(x, y, gtk::PickFlags::DEFAULT)?;
    if widget == *view {
        return Some(empty.clone());
    }
    loop {
        if is_cell_menu_widget(menus, &widget) {
            if widget
                .downcast_ref::<crate::ui::cell_editor::CellEditor>()
                .is_some_and(|editor| editor.is_editing())
            {
                return None;
            }
            let options = cell_menu_options(menus, &widget)?;
            return Some(prepare_cell_menu(menus, view, &widget, &options));
        }
        widget = widget.parent()?;
        if widget == *view {
            return None;
        }
    }
}
