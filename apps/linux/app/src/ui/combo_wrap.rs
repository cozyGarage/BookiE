use gtk4 as gtk;
use gtk4::pango;
use libadwaita as adw;
use libadwaita::prelude::*;

pub(crate) fn wrap_dialog(dialog: &adw::Dialog) {
    if let Some(content) = dialog.child() {
        wrap_all(&content);
    }
}

pub(crate) fn wrap_all(root: &impl IsA<gtk::Widget>) {
    let mut child = root.as_ref().first_child();
    while let Some(widget) = child {
        if let Some(row) = widget.downcast_ref::<adw::ComboRow>() {
            wrap(row);
        } else {
            wrap_all(&widget);
        }
        child = widget.next_sibling();
    }
}

pub(crate) fn wrap(row: &adw::ComboRow) {
    let holds_strings = row
        .model()
        .and_then(|model| model.item(0))
        .is_some_and(|item| item.downcast_ref::<gtk::StringObject>().is_some());
    if !holds_strings || row.uses_subtitle() {
        return;
    }
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            let label = gtk::Label::builder()
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .xalign(0.0)
                .build();
            item.set_child(Some(&label));
        }
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk::StringObject>()
            .map(|text| text.string());
        if let (Some(label), Some(text)) = (item.child().and_downcast::<gtk::Label>(), text) {
            label.set_label(&text);
        }
    });
    row.set_factory(Some(&factory));
    row.set_list_factory(Some(&factory));
}
