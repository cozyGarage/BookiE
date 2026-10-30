use std::rc::Rc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tokio_util::sync::CancellationToken;

use super::{build, connect_selection, schedule_zoom_update};
use crate::ui::diagram::canvas::DiagramCanvas;

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_zoom_refresh_tick_does_not_retain_its_dialog_owner() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let token = CancellationToken::new();
    let ui = Rc::new(build("Tables", &token));
    let weak_ui = Rc::downgrade(&ui);

    schedule_zoom_update(&ui);
    drop(ui);

    assert!(weak_ui.upgrade().is_none());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_diagram_canvas_releases_its_controllers() {
    gtk::init().unwrap();
    let canvas = DiagramCanvas::new();
    let weak = canvas.downgrade();
    drop(canvas);
    assert!(weak.upgrade().is_none());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn diagram_selection_callbacks_do_not_retain_the_list_or_canvas() {
    gtk::init().unwrap();
    let list = gtk::ListBox::new();
    let canvas = DiagramCanvas::new();
    let weak_list = list.downgrade();
    let weak_canvas = canvas.downgrade();

    connect_selection(&list, &canvas);
    drop(list);
    drop(canvas);

    assert!(weak_list.upgrade().is_none());
    assert!(weak_canvas.upgrade().is_none());
}
