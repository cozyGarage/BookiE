use relm4::ComponentSender;
use relm4::gtk;
use relm4::gtk::prelude::*;

use super::{EditorPane, SqlEditor, SqlEditorInput};

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
        crate::tr!("Toggle bookmark"),
        crate::tr!("Bookmark or clear the current line"),
        || SqlEditorInput::ToggleBookmark,
    ));
    content.append(&action(
        sender,
        crate::tr!("Next bookmark"),
        crate::tr!("Jump to the next bookmarked line"),
        || SqlEditorInput::JumpBookmark {
            forward: true,
            pane: EditorPane::Primary,
        },
    ));
    content.append(&action(
        sender,
        crate::tr!("Previous bookmark"),
        crate::tr!("Jump to the previous bookmarked line"),
        || SqlEditorInput::JumpBookmark {
            forward: false,
            pane: EditorPane::Primary,
        },
    ));
    content.append(&action(
        sender,
        crate::tr!("Clear bookmarks"),
        crate::tr!("Remove every bookmark in this editor"),
        || SqlEditorInput::ClearBookmarks,
    ));
    let menu = gtk::MenuButton::builder()
        .icon_name("bookmark-new-symbolic")
        .tooltip_text(crate::tr!("SQL line bookmarks"))
        .popover(&gtk::Popover::builder().child(&content).build())
        .build();
    menu.add_css_class("flat");
    menu.update_property(&[gtk::accessible::Property::Label(&crate::tr!("SQL line bookmarks"))]);
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
