use relm4::ComponentSender;
use relm4::gtk;
use relm4::gtk::prelude::*;

use super::{SqlEditor, SqlEditorInput};

pub(super) fn build(sender: &ComponentSender<SqlEditor>) -> gtk::MenuButton {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    content.append(&action(
        sender,
        crate::tr!("Fold block"),
        crate::tr!("Fold the SQL block at the cursor"),
        || SqlEditorInput::ToggleFold,
    ));
    content.append(&action(
        sender,
        crate::tr!("Unfold all"),
        crate::tr!("Show all folded SQL blocks"),
        || SqlEditorInput::UnfoldAll,
    ));
    let menu = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text(crate::tr!("Fold or unfold SQL blocks"))
        .popover(&gtk::Popover::builder().child(&content).build())
        .build();
    menu.add_css_class("flat");
    menu.update_property(&[gtk::accessible::Property::Label(&crate::tr!("Fold SQL blocks"))]);
    menu
}

fn action(
    sender: &ComponentSender<SqlEditor>,
    label: String,
    tooltip: String,
    input: fn() -> SqlEditorInput,
) -> gtk::Button {
    let button = gtk::Button::builder().label(label).tooltip_text(tooltip).build();
    button.add_css_class("flat");
    let sender = sender.clone();
    button.connect_clicked(move |_| sender.input(input()));
    button
}
