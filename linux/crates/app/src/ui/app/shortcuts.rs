use relm4::adw::prelude::*;
use relm4::gtk::gio;
use relm4::{ComponentSender, adw, gtk};

use super::{App, AppMsg};

pub(super) fn primary_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    let connection_section = gio::Menu::new();
    let disconnect_item = gio::MenuItem::new(Some(&crate::tr!("Disconnect")), Some("win.disconnect"));
    disconnect_item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    connection_section.append_item(&disconnect_item);
    menu.append_section(None, &connection_section);
    let query_section = gio::Menu::new();
    query_section.append(Some(&crate::tr!("Open Quickly")), Some("win.open-quickly"));
    query_section.append(Some(&crate::tr!("Jump to Column…")), Some("win.jump-column"));
    query_section.append(Some(&crate::tr!("Save Query as Favorite")), Some("win.save-favorite"));
    menu.append_section(None, &query_section);
    let history_section = gio::Menu::new();
    history_section.append(Some(&crate::tr!("Query History")), Some("win.show-history"));
    history_section.append(Some(&crate::tr!("Server activity")), Some("win.show-activity"));
    history_section.append(Some(&crate::tr!("Explain query")), Some("win.explain-query"));
    menu.append_section(None, &history_section);
    let prefs_section = gio::Menu::new();
    prefs_section.append(Some(&crate::tr!("Preferences")), Some("win.preferences"));
    prefs_section.append(Some(&crate::tr!("New Window")), Some("win.new-window"));
    menu.append_section(None, &prefs_section);
    let app_section = gio::Menu::new();
    app_section.append(Some(&crate::tr!("Keyboard Shortcuts")), Some("win.shortcuts"));
    app_section.append(Some(&crate::tr!("About BookiE")), Some("win.about"));
    app_section.append(Some(&crate::tr!("Quit")), Some("win.quit"));
    menu.append_section(None, &app_section);
    menu
}

pub(super) fn install_window_actions(
    window: &adw::ApplicationWindow,
    sender: ComponentSender<App>,
) -> gio::SimpleAction {
    let group = gio::SimpleActionGroup::new();
    macro_rules! input_action {
        ($name:expr, $msg:expr) => {{
            let s = sender.clone();
            gio::ActionEntry::builder($name)
                .activate(move |_, _, _| s.input($msg))
                .build()
        }};
    }
    let window_for_quit = window.clone();
    let quit = gio::ActionEntry::builder("quit")
        .activate(move |_, _, _| window_for_quit.close())
        .build();
    group.add_action_entries([
        input_action!("shortcuts", AppMsg::ShowShortcuts),
        input_action!("about", AppMsg::ShowAbout),
        quit,
        input_action!("open-editor", AppMsg::NewEditorTab),
        input_action!("open-file", AppMsg::OpenSqlFile),
        input_action!("save-file-as", AppMsg::SaveActiveEditorFileAs),
        input_action!("close-current", AppMsg::CloseActiveWorkspaceTab),
        input_action!("preferences", AppMsg::ShowPreferences),
        input_action!("new-window", AppMsg::NewWindow),
        input_action!("show-history", AppMsg::ShowHistory),
        input_action!("show-activity", AppMsg::ShowActivity),
        input_action!("explain-query", AppMsg::ExplainActiveQuery),
        input_action!("refresh-page", AppMsg::RefreshPage),
        input_action!("jump-column", AppMsg::JumpToColumn),
        input_action!("export-csv", AppMsg::ExportCsv),
        input_action!("export-json", AppMsg::ExportJson),
        input_action!("save-changes", AppMsg::SaveActiveBrowseTab),
        input_action!("undo-change", AppMsg::UndoActiveBrowseTab),
        input_action!("redo-change", AppMsg::RedoActiveBrowseTab),
        input_action!("reopen-closed-tab", AppMsg::ReopenClosedTab),
        input_action!("open-filter", AppMsg::ShowFilterDialog),
        input_action!("open-quickly", AppMsg::ShowQuickSwitcher),
        input_action!("save-favorite", AppMsg::SaveQueryAsFavorite),
        input_action!("show-saved-queries", AppMsg::ShowSavedQueries),
    ]);
    let disconnect_action = gio::SimpleAction::new("disconnect", None);
    let disconnect_sender = sender.clone();
    disconnect_action.connect_activate(move |_, _| disconnect_sender.input(AppMsg::Disconnect));
    group.add_action(&disconnect_action);
    window.insert_action_group("win", Some(&group));
    disconnect_action.set_enabled(false);
    tracing::info!(enabled = disconnect_action.is_enabled(), "registered win.disconnect");
    disconnect_action
}

pub(super) fn install_window_shortcuts(window: &adw::ApplicationWindow) {
    let controller = gtk::ShortcutController::new();
    controller.set_scope(gtk::ShortcutScope::Global);
    for (trigger, action) in [
        ("<Primary>question", "win.shortcuts"),
        ("<Primary>slash", "win.shortcuts"),
        ("<Primary>q", "win.quit"),
        ("<Primary>w", "win.close-current"),
        ("<Primary>e", "win.open-editor"),
        ("<Primary>t", "win.open-editor"),
        ("<Primary>o", "win.open-file"),
        ("F5", "win.refresh-page"),
        ("<Primary>f", "win.open-filter"),
        ("<Primary>comma", "win.preferences"),
        ("<Primary>h", "win.show-history"),
        ("<Primary>p", "win.open-quickly"),
        ("<Primary><Shift>j", "win.jump-column"),
        ("<Primary>d", "win.save-favorite"),
        ("<Primary><Shift>d", "win.show-saved-queries"),
        ("<Primary>s", "win.save-changes"),
        ("<Primary><Shift>s", "win.save-file-as"),
        ("<Primary>z", "win.undo-change"),
        ("<Primary>y", "win.redo-change"),
        ("<Primary><Shift>z", "win.redo-change"),
        ("<Primary><Shift>t", "win.reopen-closed-tab"),
    ] {
        controller.add_shortcut(make_shortcut(trigger, action));
    }
    window.add_controller(controller);
}

fn make_shortcut(trigger: &str, action: &str) -> gtk::Shortcut {
    gtk::Shortcut::builder()
        .trigger(&crate::ui::shortcut::parse(trigger))
        .action(&gtk::NamedAction::new(action))
        .build()
}

pub(super) fn build_shortcuts_dialog() -> adw::ShortcutsDialog {
    let dialog = adw::ShortcutsDialog::new();
    add_shortcut_section(
        &dialog,
        &crate::tr!("General"),
        &[
            ("<Primary>e", crate::tr!("Open SQL editor")),
            ("<Primary>o", crate::tr!("Open a SQL file in a new editor tab")),
            ("F5", crate::tr!("Refresh table")),
            ("<Primary>comma", crate::tr!("Open Preferences")),
            ("<Primary>h", crate::tr!("Open Query History")),
            ("<Primary>p", crate::tr!("Open Quickly: favorites and open tabs")),
            ("<Primary>d", crate::tr!("Save the editor query as a favorite")),
            ("<Primary><Shift>d", crate::tr!("Manage saved queries")),
            ("<Primary>s", crate::tr!("Save pending changes")),
            ("<Primary>z", crate::tr!("Undo pending change")),
            ("<Primary>y", crate::tr!("Redo pending change")),
            ("<Primary>question", crate::tr!("Show keyboard shortcuts")),
            ("<Primary>q", crate::tr!("Quit")),
        ],
    );
    add_shortcut_section(
        &dialog,
        &crate::tr!("Browse table"),
        &[
            ("F2", crate::tr!("Edit focused cell")),
            ("Return", crate::tr!("Edit focused cell")),
            ("Escape", crate::tr!("Cancel edit")),
            ("Tab", crate::tr!("Move to next cell (commits if editing)")),
            ("<Shift>Tab", crate::tr!("Move to previous cell (commits if editing)")),
            ("Left", crate::tr!("Move to previous cell")),
            ("Right", crate::tr!("Move to next cell")),
            ("space", crate::tr!("Toggle boolean cell")),
            ("<Primary>n", crate::tr!("Insert row")),
            ("Delete", crate::tr!("Delete selected row")),
            ("<Primary><Shift>n", crate::tr!("Set focused cell to NULL")),
            ("<Primary>f", crate::tr!("Filter rows")),
            ("<Primary><Shift>j", crate::tr!("Jump to Column (focused grid)")),
            ("<Primary>a", crate::tr!("Select all rows")),
            (
                "<Shift>Pointer_Button1",
                crate::tr!("Extend row selection to clicked row"),
            ),
            (
                "<Primary>Pointer_Button1",
                crate::tr!("Toggle clicked row in selection"),
            ),
            ("Escape", crate::tr!("Clear multi-row selection")),
            ("<Primary>c", crate::tr!("Copy selected rows as TSV")),
            ("Page_Up", crate::tr!("Previous page")),
            ("Page_Down", crate::tr!("Next page")),
            ("<Primary>Home", crate::tr!("Jump to first row of page")),
            ("<Primary>End", crate::tr!("Jump to last row of page")),
            ("<Primary>s", crate::tr!("Save pending edits")),
            ("<Primary>z", crate::tr!("Undo last change")),
            ("<Primary><Shift>z", crate::tr!("Redo last change")),
        ],
    );
    add_shortcut_section(
        &dialog,
        &crate::tr!("SQL editor"),
        &[
            ("<Primary>Return", crate::tr!("Run query")),
            ("<Primary><Shift>Return", crate::tr!("Run statement at cursor")),
            ("<Primary>s", crate::tr!("Save the SQL file")),
            ("<Primary><Shift>s", crate::tr!("Save the SQL file as")),
            ("Escape", crate::tr!("Cancel running query")),
            ("<Primary>slash", crate::tr!("Toggle line comment")),
            ("<Primary>t", crate::tr!("New editor tab")),
            ("<Primary>w", crate::tr!("Close current tab or window")),
            ("<Primary>Tab", crate::tr!("Next editor tab")),
            ("<Primary><Shift>Tab", crate::tr!("Previous editor tab")),
            ("<Primary><Shift>t", crate::tr!("Reopen last closed tab")),
            ("<Primary><Shift>f", crate::tr!("Format SQL")),
        ],
    );
    add_shortcut_section(
        &dialog,
        &crate::tr!("Table structure"),
        &[
            ("<Primary>s", crate::tr!("Save pending DDL")),
            ("<Primary>z", crate::tr!("Undo DDL change")),
            ("<Primary><Shift>z", crate::tr!("Redo DDL change")),
        ],
    );
    add_shortcut_section(
        &dialog,
        &crate::tr!("Dialogs"),
        &[("Escape", crate::tr!("Close dialog"))],
    );
    dialog
}

fn add_shortcut_section(dialog: &adw::ShortcutsDialog, title: &str, entries: &[(&str, String)]) {
    let section = adw::ShortcutsSection::new(Some(title));
    for (accelerator, title) in entries {
        section.add(adw::ShortcutsItem::new(title, accelerator));
    }
    dialog.add(section);
}
