use super::*;

pub(super) struct FilterRows {
    pub connection: adw::ComboRow,
    pub status: adw::ComboRow,
    pub window: adw::ComboRow,
    pub source: adw::ComboRow,
}

pub(super) struct ListArea {
    pub scroll: gtk::ScrolledWindow,
    pub pinned_group: adw::PreferencesGroup,
    pub pinned_listbox: gtk::ListBox,
    pub list_group: adw::PreferencesGroup,
    pub listbox: gtk::ListBox,
}

pub(super) struct SelectionBar {
    pub revealer: gtk::Revealer,
    pub label: gtk::Label,
}

pub(super) fn close_button(sender: &ComponentSender<HistoryDialog>) -> gtk::Button {
    let button = gtk::Button::builder()
        .label(crate::tr!("Close history"))
        .tooltip_text(crate::tr!("Close history"))
        .build();
    let close_sender = sender.clone();
    button.connect_clicked(move |_| {
        let _ = close_sender.output(HistoryDialogOutput::Close);
    });
    button
}

fn combo_row(title: String, options: &[String]) -> adw::ComboRow {
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(&options))
        .build()
}

fn filter_rows(connections: &[ConnectionMetadata]) -> FilterRows {
    let connection_names: Vec<String> = std::iter::once(crate::tr!("All connections"))
        .chain(connections.iter().map(|m| m.name.clone()))
        .collect();
    FilterRows {
        connection: combo_row(crate::tr!("Connection"), &connection_names),
        status: combo_row(
            crate::tr!("Status"),
            &[
                crate::tr!("Any"),
                crate::tr!("Successful"),
                crate::tr!("Failed"),
                crate::tr!("Cancelled"),
            ],
        ),
        window: combo_row(
            crate::tr!("Time window"),
            &[
                crate::tr!("Any time"),
                crate::tr!("Last 24 hours"),
                crate::tr!("Last 7 days"),
                crate::tr!("Last 30 days"),
            ],
        ),
        source: combo_row(
            crate::tr!("Ran from"),
            &[
                crate::tr!("Anywhere"),
                crate::tr!("SQL editor"),
                crate::tr!("Table structure"),
                crate::tr!("Row edits"),
            ],
        ),
    }
}

fn reset_button(rows: &FilterRows) -> gtk::Button {
    let button = gtk::Button::builder()
        .label(crate::tr!("Reset"))
        .halign(gtk::Align::End)
        .margin_top(6)
        .build();
    button.add_css_class("flat");
    let combos = [
        rows.connection.clone(),
        rows.status.clone(),
        rows.window.clone(),
        rows.source.clone(),
    ];
    button.connect_clicked(move |_| {
        for combo in &combos {
            combo.set_selected(0);
        }
    });
    button
}

/// The header's Filter button with its popover of four combo rows and a Reset button.
pub(super) fn filter_button(
    connections: &[ConnectionMetadata],
    sender: &ComponentSender<HistoryDialog>,
) -> (gtk::MenuButton, FilterRows) {
    let popover = gtk::Popover::new();
    let button = gtk::MenuButton::builder()
        .label(crate::tr!("Filter"))
        .always_show_arrow(true)
        .tooltip_text(crate::tr!("Filter history"))
        .popover(&popover)
        .build();
    keep_inside_window(&popover, &button);

    let rows = filter_rows(connections);
    let reset = reset_button(&rows);

    // Native popover content: a single AdwPreferencesGroup of AdwComboRow
    // filters, with Reset in the group's header-suffix slot so no wrapper Box
    // is needed for layout.
    let group = adw::PreferencesGroup::new();
    group.add(&rows.connection);
    group.add(&rows.status);
    group.add(&rows.window);
    group.add(&rows.source);
    group.set_header_suffix(Some(&reset));
    popover.set_child(Some(&group));

    for combo in [&rows.connection, &rows.status, &rows.window, &rows.source] {
        let s = sender.clone();
        combo.connect_selected_notify(move |_| s.input(HistoryDialogInput::FiltersChanged));
    }
    (button, rows)
}

pub(super) fn select_button(sender: &ComponentSender<HistoryDialog>) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder()
        .label(crate::tr!("Select"))
        .tooltip_text(crate::tr!("Toggle multi-select"))
        .build();
    let s_select = sender.clone();
    button.connect_toggled(move |btn| {
        s_select.input(HistoryDialogInput::ToggleSelectMode(btn.is_active()));
    });
    button
}

pub(super) fn more_actions_button() -> gtk::MenuButton {
    let menu = gio::Menu::new();
    let storage_section = gio::Menu::new();
    storage_section.append(Some(&crate::tr!("Show storage location")), Some("history.show-storage"));
    menu.append_section(None, &storage_section);
    let danger_section = gio::Menu::new();
    danger_section.append(Some(&crate::tr!("Clear all history…")), Some("history.clear-all"));
    menu.append_section(None, &danger_section);
    gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .menu_model(&menu)
        .tooltip_text(crate::tr!("More actions"))
        .build()
}

fn action(
    name: &str,
    sender: &ComponentSender<HistoryDialog>,
    input: fn() -> HistoryDialogInput,
) -> gio::ActionEntry<gio::SimpleActionGroup> {
    let sender = sender.clone();
    gio::ActionEntry::builder(name)
        .activate(move |_, _, _| sender.input(input()))
        .build()
}

pub(super) fn action_group(sender: &ComponentSender<HistoryDialog>) -> gio::SimpleActionGroup {
    let group = gio::SimpleActionGroup::new();
    group.add_action_entries([
        action("export-sql", sender, || HistoryDialogInput::ExportSelectedSql),
        action("export-csv", sender, || HistoryDialogInput::ExportSelectedCsv),
        action("delete-selected", sender, || HistoryDialogInput::DeleteSelected),
        action("clear-all", sender, || HistoryDialogInput::ClearAllRequested),
        action("show-storage", sender, || HistoryDialogInput::OpenStorageLocation),
    ]);
    group
}

/// The search entry in its bar, plus the header toggle that shows the bar.
pub(super) fn search(
    sender: &ComponentSender<HistoryDialog>,
) -> (gtk::SearchEntry, gtk::SearchBar, gtk::ToggleButton) {
    let entry = gtk::SearchEntry::builder()
        .placeholder_text(crate::tr!("Search queries…"))
        .hexpand(true)
        .build();
    let s = sender.clone();
    entry.connect_search_changed(move |_| s.input(HistoryDialogInput::SearchChanged));

    let bar = gtk::SearchBar::builder()
        .child(&entry)
        .show_close_button(true)
        .search_mode_enabled(false)
        .build();
    bar.connect_entry(&entry);

    let toggle = gtk::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text(crate::tr!("Search"))
        .build();
    let bar_for_toggle = bar.clone();
    toggle.connect_toggled(move |btn| {
        bar_for_toggle.set_search_mode(btn.is_active());
    });
    let toggle_for_close = toggle.clone();
    bar.connect_search_mode_enabled_notify(move |bar| {
        toggle_for_close.set_active(bar.is_search_mode());
    });
    (entry, bar, toggle)
}

fn boxed_list() -> gtk::ListBox {
    let listbox = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).build();
    listbox.add_css_class("boxed-list");
    listbox
}

/// Each section is an AdwPreferencesGroup with a boxed-list ListBox; the
/// group's visibility is toggled instead of separate Label + ListBox.
fn section(title: String) -> (adw::PreferencesGroup, gtk::ListBox) {
    let listbox = boxed_list();
    let group = adw::PreferencesGroup::builder().title(title).visible(false).build();
    group.add(&listbox);
    (group, listbox)
}

pub(super) fn list_area() -> ListArea {
    let (pinned_group, pinned_listbox) = section(crate::tr!("Pinned"));
    let (list_group, listbox) = section(crate::tr!("All queries"));

    let list_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    list_box.append(&pinned_group);
    list_box.append(&list_group);

    let scroll = gtk::ScrolledWindow::builder()
        .child(&list_box)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    ListArea {
        scroll,
        pinned_group,
        pinned_listbox,
        list_group,
        listbox,
    }
}

/// The stack that shows the list, or the empty-state page before any query ran.
pub(super) fn content_stack(scroll: &gtk::ScrolledWindow) -> (gtk::Stack, adw::StatusPage) {
    let status_page = adw::StatusPage::builder()
        .icon_name("document-open-recent-symbolic")
        .title(crate::tr!("No queries yet"))
        .description(crate::tr!("Run a query in the SQL editor and it will appear here."))
        .vexpand(true)
        .build();
    let stack = gtk::Stack::builder().vhomogeneous(false).vexpand(true).margin_top(12).build();
    stack.add_named(scroll, Some("list"));
    stack.add_named(&status_page, Some("empty"));
    (stack, status_page)
}

pub(super) fn selection_bar(sender: &ComponentSender<HistoryDialog>) -> SelectionBar {
    let label = gtk::Label::builder().xalign(0.0).hexpand(true).build();
    label.add_css_class("dim-label");

    let export_menu = gio::Menu::new();
    export_menu.append(Some(&crate::tr!("Export as SQL")), Some("history.export-sql"));
    export_menu.append(Some(&crate::tr!("Export as CSV")), Some("history.export-csv"));
    let export_button = gtk::MenuButton::builder()
        .label(crate::tr!("Export"))
        .always_show_arrow(true)
        .menu_model(&export_menu)
        .build();

    let delete_button = gtk::Button::builder().label(crate::tr!("Delete")).build();
    delete_button.add_css_class("destructive-action");
    let s_del_bar = sender.clone();
    delete_button.connect_clicked(move |_| s_del_bar.input(HistoryDialogInput::DeleteSelected));

    let inner = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(12)
        .margin_end(12)
        .build();
    inner.append(&label);
    inner.append(&export_button);
    inner.append(&delete_button);
    let revealer = gtk::Revealer::builder()
        .child(&inner)
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .reveal_child(false)
        .build();
    SelectionBar { revealer, label }
}
