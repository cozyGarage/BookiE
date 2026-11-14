use super::*;

pub(super) fn row_shell(entry: &Entry) -> adw::ActionRow {
    let title = preview_title(&entry.query);
    let subtitle = format_subtitle(entry);
    let row = adw::ActionRow::builder()
        .use_markup(false)
        .title(&title)
        .subtitle(&subtitle)
        .activatable(true)
        .build();
    row.add_css_class("monospace");

    let icon_name = if entry.cancelled {
        "process-stop-symbolic"
    } else if entry.success {
        "emblem-ok-symbolic"
    } else {
        "dialog-error-symbolic"
    };
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("dim-label");
    row.add_prefix(&icon);
    row
}

pub(super) fn select_check(
    entry: &Entry,
    select_mode: bool,
    selected: bool,
    sender: &ComponentSender<HistoryDialog>,
) -> gtk::CheckButton {
    let select_check = gtk::CheckButton::builder()
        .valign(gtk::Align::Center)
        .visible(select_mode)
        .build();
    select_check.set_active(selected);
    let id_for_check = entry.id;
    let s_check = sender.clone();
    select_check.connect_toggled(move |btn| {
        s_check.input(HistoryDialogInput::ToggleSelected(id_for_check, btn.is_active()));
    });
    select_check
}

pub(super) fn pin_button(entry: &Entry, sender: &ComponentSender<HistoryDialog>) -> gtk::Button {
    let pin_btn = gtk::Button::builder()
        .icon_name("view-pin-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text(if entry.pinned {
            bookie_app_services::tr!("Unpin")
        } else {
            bookie_app_services::tr!("Pin")
        })
        .build();
    pin_btn.add_css_class("flat");
    if entry.pinned {
        pin_btn.add_css_class("accent");
    }
    let id = entry.id;
    let s = sender.clone();
    pin_btn.connect_clicked(move |_| s.input(HistoryDialogInput::TogglePin(id)));
    pin_btn
}

pub(super) fn delete_button(entry: &Entry, sender: &ComponentSender<HistoryDialog>) -> gtk::Button {
    let id = entry.id;
    let delete_btn = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text(bookie_app_services::tr!("Delete"))
        .build();
    delete_btn.add_css_class("flat");
    delete_btn.add_css_class("destructive-action");
    let s_del = sender.clone();
    delete_btn.connect_clicked(move |_| s_del.input(HistoryDialogInput::Delete(id)));
    delete_btn
}

pub(super) fn row_menu(entry: &Entry) -> gio::Menu {
    let menu = gio::Menu::new();
    let nav_section = gio::Menu::new();
    nav_section.append(
        Some(&bookie_app_services::tr!("Open in new tab")),
        Some("history-row.open"),
    );
    nav_section.append(
        Some(&bookie_app_services::tr!("Replace current tab")),
        Some("history-row.replace"),
    );
    menu.append_section(None, &nav_section);
    let util_section = gio::Menu::new();
    util_section.append(
        Some(&if entry.pinned {
            bookie_app_services::tr!("Unpin")
        } else {
            bookie_app_services::tr!("Pin")
        }),
        Some("history-row.pin"),
    );
    util_section.append(Some(&bookie_app_services::tr!("Copy SQL")), Some("history-row.copy"));
    menu.append_section(None, &util_section);
    let danger_section = gio::Menu::new();
    danger_section.append(Some(&bookie_app_services::tr!("Delete")), Some("history-row.delete"));
    menu.append_section(None, &danger_section);
    menu
}

pub(super) fn row_actions(
    entry: &Entry,
    row: &adw::ActionRow,
    sender: &ComponentSender<HistoryDialog>,
) -> gio::SimpleActionGroup {
    let id = entry.id;
    let row_actions = gio::SimpleActionGroup::new();
    let s_open = sender.clone();
    let act_open = gio::ActionEntry::builder("open")
        .activate(move |_, _, _| s_open.input(HistoryDialogInput::Activate(id)))
        .build();
    let s_replace = sender.clone();
    let act_replace = gio::ActionEntry::builder("replace")
        .activate(move |_, _, _| s_replace.input(HistoryDialogInput::ReplaceCurrent(id)))
        .build();
    let s_pin = sender.clone();
    let act_pin = gio::ActionEntry::builder("pin")
        .activate(move |_, _, _| s_pin.input(HistoryDialogInput::TogglePin(id)))
        .build();
    let query_for_copy = entry.query.clone();
    let row_for_copy = row.downgrade();
    let act_copy = gio::ActionEntry::builder("copy")
        .activate(move |_, _, _| {
            if let Some(row) = row_for_copy.upgrade() {
                row.clipboard().set_text(&query_for_copy);
            }
        })
        .build();
    let s_del2 = sender.clone();
    let act_del = gio::ActionEntry::builder("delete")
        .activate(move |_, _, _| s_del2.input(HistoryDialogInput::Delete(id)))
        .build();
    row_actions.add_action_entries([act_open, act_replace, act_pin, act_copy, act_del]);
    row_actions
}

pub(super) fn attach_context_menu(row: &adw::ActionRow, popover_menu: &gtk::PopoverMenu) {
    let gesture = gtk::GestureClick::new();
    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
    let popover_for_gesture = popover_menu.clone();
    gesture.connect_pressed(move |gesture, _n, x, y| {
        crate::ui::context_menu::popup_at(
            &popover_for_gesture,
            Some(gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)),
        );
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    row.add_controller(gesture);

    let menu_shortcut = gtk::Shortcut::builder()
        .trigger(&crate::ui::shortcut::parse("Menu"))
        .action(&gtk::CallbackAction::new({
            let popover = popover_menu.clone();
            let row = row.downgrade();
            move |_, _| {
                let pointing = row.upgrade().and_then(|row| {
                    row.compute_bounds(&row).map(|bounds| {
                        gtk::gdk::Rectangle::new((bounds.width() / 2.0) as i32, (bounds.height() / 2.0) as i32, 1, 1)
                    })
                });
                crate::ui::context_menu::popup_at(&popover, pointing);
                glib::Propagation::Stop
            }
        }))
        .build();
    let controller = gtk::ShortcutController::new();
    controller.set_scope(gtk::ShortcutScope::Local);
    controller.add_shortcut(menu_shortcut);
    row.add_controller(controller);
}
