use super::*;

#[test]
#[ignore = "requires an isolated GTK display"]
fn dropping_the_colour_chooser_releases_its_popover() {
    adw::init().unwrap();
    let (row, selected) = color_chooser(0);
    let popup = find_popover(row.upcast_ref()).unwrap();
    let choices = popup.child().unwrap();
    let blue = choices
        .first_child()
        .unwrap()
        .next_sibling()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    blue.emit_clicked();
    assert_eq!(selected.get(), 1);
    drop(blue);
    drop(choices);
    drop(popup);
    let popover = find_popover(row.upcast_ref()).unwrap().downgrade();
    drop(row);
    let context = gtk::glib::MainContext::default();
    while context.iteration(false) {}
    assert!(
        popover.upgrade().is_none(),
        "colour popover remains after its chooser is dropped"
    );
}

fn find_popover(widget: &gtk::Widget) -> Option<gtk::Popover> {
    if let Some(popover) = widget.downcast_ref::<gtk::Popover>() {
        return Some(popover.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(popover) = find_popover(&current) {
            return Some(popover);
        }
    }
    None
}
