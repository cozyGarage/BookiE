use relm4::gtk;
use relm4::gtk::prelude::*;

pub(super) fn keep_inside_window(popover: &gtk::Popover, button: &gtk::MenuButton) {
    let button = button.downgrade();
    popover.connect_show(move |popover| {
        let Some(button) = button.upgrade() else { return };
        let Some(window) = button.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let Some(bounds) = button.compute_bounds(&window) else {
            return;
        };
        let (_, width, _, _) = popover.measure(gtk::Orientation::Horizontal, -1);
        let center = bounds.x() + bounds.width() / 2.0;
        let natural_left = center - width as f32 / 2.0;
        let max_left = (window.width() - width).max(0) as f32;
        let offset = (natural_left.clamp(0.0, max_left) - natural_left).round() as i32;
        popover.set_offset(offset, 0);
    });
}
