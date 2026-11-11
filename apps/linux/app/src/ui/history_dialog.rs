use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use relm4::adw::prelude::*;
use relm4::gtk::{gio, glib};
use relm4::prelude::*;
use relm4::{adw, gtk};

use bookie_storage::query_history::{Entry, HistoryStore, SearchFilter, Source};

use crate::services::database_service::ConnectionMetadata;
use crate::ui::popover::keep_inside_window;

pub struct HistoryDialog {
    history: HistoryStore,
    root: gtk::Box,
    search: gtk::SearchEntry,
    pinned_group: adw::PreferencesGroup,
    pinned_listbox: gtk::ListBox,
    list_group: adw::PreferencesGroup,
    listbox: gtk::ListBox,
    stack: gtk::Stack,
    status_page: adw::StatusPage,

    filter_connection: adw::ComboRow,
    filter_status: adw::ComboRow,
    filter_window: adw::ComboRow,
    filter_source: adw::ComboRow,
    /// Pending debounced search timeout. Replaces the previous fire-
    /// on-every-keystroke behaviour: typing or flipping a filter
    /// schedules a Refresh 150 ms later and cancels any earlier
    /// scheduled one, so we send a single SQL search per pause —
    /// matches GNOME Files' search debounce.
    filter_debounce: std::rc::Rc<std::cell::RefCell<Option<gtk::glib::SourceId>>>,
    search_generation: std::cell::Cell<u64>,
    mutation_order: std::sync::Arc<tokio::sync::Mutex<()>>,

    selection_bar: gtk::Revealer,
    selection_label: gtk::Label,

    connections: Vec<ConnectionMetadata>,
    selected_ids: HashSet<i64>,
    entries: Vec<Entry>,
    row_popovers: Vec<gtk::PopoverMenu>,
    row_checkboxes: Vec<gtk::CheckButton>,
    select_mode: bool,
}

pub struct HistoryDialogInit {
    pub history: HistoryStore,
    pub database: std::sync::Arc<crate::services::database_service::DatabaseService>,
}

#[derive(Debug)]
pub enum HistoryDialogInput {
    SearchChanged,
    FiltersChanged,
    Refresh,
    Activate(i64),
    ReplaceCurrent(i64),
    TogglePin(i64),
    Delete(i64),
    ToggleSelectMode(bool),
    ToggleSelected(i64, bool),
    DeleteSelected,
    /// Confirmed-bulk-delete arm. Bulk deletion is irreversible
    /// (pinned entries included), so the inline `Delete` button on
    /// the selection bar opens an `AdwAlertDialog` first; only the
    /// destructive response routes to this variant.
    DeleteSelectedConfirmed,
    ExportSelectedSql,
    ExportSelectedCsv,
    ClearAllRequested,
    ClearAllConfirmed,
    OpenStorageLocation,
}

#[derive(Debug)]
pub enum HistoryDialogOutput {
    OpenInNewTab(String),
    ReplaceCurrentTabQuery(String),
    Close,
}

#[derive(Debug)]
pub enum HistoryDialogCmd {
    Loaded(u64, Vec<Entry>),
    Cleared,
    ExportReady(String, String),
    ExportFailed(String),
    ExportWritten(Result<(), String>),
}

impl Component for HistoryDialog {
    type Init = HistoryDialogInit;
    type Input = HistoryDialogInput;
    type Output = HistoryDialogOutput;
    type CommandOutput = HistoryDialogCmd;
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::builder().orientation(gtk::Orientation::Vertical).build()
    }

    fn init(init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
        header.set_title_widget(Some(&adw::WindowTitle::new(&bookie_app_services::tr!("Query History"), "")));
        header.pack_end(&build::close_button(&sender));

        let connections = init.database.all_connections();
        let (filter_button, filters) = build::filter_button(&connections, &sender);
        header.pack_start(&filter_button);
        header.pack_start(&build::select_button(&sender));
        header.pack_end(&build::more_actions_button());

        let (search, search_bar, search_toggle) = build::search(&sender);
        header.pack_end(&search_toggle);

        let inner = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        inner.append(&search_bar);
        let lists = build::list_area();
        let (stack, status_page) = build::content_stack(&lists.scroll);
        inner.append(&stack);
        search_bar.set_key_capture_widget(Some(&inner));

        let selection = build::selection_bar(&sender);
        toolbar.add_bottom_bar(&selection.revealer);
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&inner));
        root.append(&toolbar);
        root.insert_action_group("history", Some(&build::action_group(&sender)));

        let model = Self {
            history: init.history,
            root: root.clone(),
            search,
            pinned_group: lists.pinned_group,
            pinned_listbox: lists.pinned_listbox,
            list_group: lists.list_group,
            listbox: lists.listbox,
            stack,
            status_page,
            filter_connection: filters.connection,
            filter_status: filters.status,
            filter_window: filters.window,
            filter_source: filters.source,
            filter_debounce: std::rc::Rc::new(std::cell::RefCell::new(None)),
            search_generation: std::cell::Cell::new(0),
            mutation_order: std::sync::Arc::default(),
            selection_bar: selection.revealer,
            selection_label: selection.label,
            connections,
            selected_ids: HashSet::new(),
            entries: Vec::new(),
            row_popovers: Vec::new(),
            row_checkboxes: Vec::new(),
            select_mode: false,
        };

        sender.input(HistoryDialogInput::Refresh);

        ComponentParts { model, widgets: () }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        if let Some(timeout) = self.filter_debounce.borrow_mut().take() {
            timeout.remove();
        }
        for popover in self.row_popovers.drain(..) {
            popover.unparent();
        }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            HistoryDialogInput::Refresh => self.run_search(sender),
            // SearchChanged + FiltersChanged go through the debounce —
            // every keystroke and every dropdown flip would otherwise
            // fire its own SQL search. After a 150 ms idle window the
            // scheduled timeout re-enters update with Refresh, which
            // does the actual work.
            HistoryDialogInput::SearchChanged | HistoryDialogInput::FiltersChanged => {
                self.schedule_debounced_search(sender);
            }

            HistoryDialogInput::Activate(id) => {
                if let Some(query) = self.query_for_id(id) {
                    let _ = sender.output(HistoryDialogOutput::OpenInNewTab(query));
                    let _ = sender.output(HistoryDialogOutput::Close);
                }
            }

            HistoryDialogInput::ReplaceCurrent(id) => {
                if let Some(query) = self.query_for_id(id) {
                    let _ = sender.output(HistoryDialogOutput::ReplaceCurrentTabQuery(query));
                    let _ = sender.output(HistoryDialogOutput::Close);
                }
            }

            HistoryDialogInput::TogglePin(id) => self.toggle_pin(id, sender),

            HistoryDialogInput::Delete(id) => self.delete_one(id, sender),

            HistoryDialogInput::ToggleSelected(id, on) => {
                if on {
                    self.selected_ids.insert(id);
                } else {
                    self.selected_ids.remove(&id);
                }
                self.refresh_selection_bar();
            }

            HistoryDialogInput::ToggleSelectMode(on) => self.set_select_mode(on),

            HistoryDialogInput::DeleteSelected => self.confirm_delete_selected(sender),

            HistoryDialogInput::DeleteSelectedConfirmed => self.delete_selected(sender),

            HistoryDialogInput::ExportSelectedSql => self.export_selected("sql", sender),
            HistoryDialogInput::ExportSelectedCsv => self.export_selected("csv", sender),

            HistoryDialogInput::ClearAllRequested => self.confirm_clear_all(sender),

            HistoryDialogInput::ClearAllConfirmed => self.clear_all(sender),

            HistoryDialogInput::OpenStorageLocation => self.open_storage_location(),
        }
    }

    fn update_cmd(&mut self, msg: Self::CommandOutput, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            HistoryDialogCmd::Loaded(generation, entries)
                if crate::services::request_generation::is_current(generation, self.search_generation.get()) =>
            {
                self.render_entries(entries, &sender)
            }
            HistoryDialogCmd::Loaded(..) => {}
            HistoryDialogCmd::Cleared => {
                self.selected_ids.clear();
                sender.input(HistoryDialogInput::Refresh);
            }
            HistoryDialogCmd::ExportReady(content, suggested_name) => self.save_export(content, suggested_name, sender),
            HistoryDialogCmd::ExportFailed(msg) | HistoryDialogCmd::ExportWritten(Err(msg)) => self.export_failed(&msg),
            HistoryDialogCmd::ExportWritten(Ok(())) => {}
        }
    }
}

impl HistoryDialog {
    /// Run the search immediately. Triggered by Refresh (initial load
    /// + after any mutation) and by the debounced timeout firing.
    fn run_search(&self, sender: ComponentSender<Self>) {
        self.run_search_after(sender, async {});
    }

    fn run_search_after(
        &self,
        sender: ComponentSender<Self>,
        mutation: impl std::future::Future<Output = ()> + Send + 'static,
    ) {
        let generation = self.search_generation.get().wrapping_add(1);
        self.search_generation.set(generation);
        let filter = self.build_filter();
        let history = self.history.clone();
        let order = self.mutation_order.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let entries = crate::services::ordered_refresh::after_mutation(&order, mutation, async {
                        history.search(filter).await.unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "history search failed");
                            Vec::new()
                        })
                    })
                    .await;
                    out.send(HistoryDialogCmd::Loaded(generation, entries)).ok()
                })
                .drop_on_shutdown()
        });
    }

    /// Cancel any pending debounce timeout and schedule a new Refresh
    /// 150 ms from now. The 150 ms idle threshold matches GNOME Files'
    /// search-typing debounce — short enough to feel responsive, long
    /// enough that a fast typer never fires more than one query.
    fn schedule_debounced_search(&self, sender: ComponentSender<Self>) {
        if let Some(prev) = self.filter_debounce.borrow_mut().take() {
            prev.remove();
        }
        let s = sender.clone();
        let slot = self.filter_debounce.clone();
        let id = gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
            slot.borrow_mut().take();
            s.input(HistoryDialogInput::Refresh);
        });
        *self.filter_debounce.borrow_mut() = Some(id);
    }

    fn build_filter(&self) -> SearchFilter {
        let needle = {
            let text = self.search.text().to_string();
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(fts5_query(&trimmed))
            }
        };
        let connection_id = if self.filter_connection.selected() == 0 {
            None
        } else {
            self.connections
                .get((self.filter_connection.selected() - 1) as usize)
                .map(|m| m.id)
        };
        let (success_only, exclude_cancelled) = match self.filter_status.selected() {
            0 => (None, None),
            1 => (Some(true), Some(true)),
            2 => (Some(false), Some(true)),
            3 => (None, Some(false)),
            _ => (None, None),
        };
        let min_executed_at = match self.filter_window.selected() {
            1 => Some(SystemTime::now() - Duration::from_secs(24 * 3600)),
            2 => Some(SystemTime::now() - Duration::from_secs(7 * 24 * 3600)),
            3 => Some(SystemTime::now() - Duration::from_secs(30 * 24 * 3600)),
            _ => None,
        };
        SearchFilter {
            needle,
            connection_id,
            success_only,
            exclude_cancelled,
            min_executed_at,
            source: source_for_index(self.filter_source.selected()),
            limit: 200,
        }
    }

    fn has_narrowing_search(&self) -> bool {
        !self.search.text().to_string().trim().is_empty()
    }

    fn has_narrowing_filter(&self) -> bool {
        self.filter_connection.selected() != 0
            || self.filter_status.selected() != 0
            || self.filter_window.selected() != 0
            || self.filter_source.selected() != 0
    }

    fn render_entries(&mut self, entries: Vec<Entry>, sender: &ComponentSender<Self>) {
        // Unparent the previous round's row popovers explicitly so they drop
        // along with their captured sender clones. Without this, set_parent's
        // strong link from popover→row would keep both alive past the
        // listbox.remove() call below.
        for popover in self.row_popovers.drain(..) {
            popover.unparent();
        }
        self.row_checkboxes.clear();
        clear_listbox(&self.pinned_listbox);
        clear_listbox(&self.listbox);

        let valid_ids: HashSet<i64> = entries.iter().map(|e| e.id).collect();
        self.selected_ids.retain(|id| valid_ids.contains(id));
        self.entries = entries.clone();

        if entries.is_empty() {
            self.pinned_group.set_visible(false);
            self.list_group.set_visible(false);
            if self.has_narrowing_search() || self.has_narrowing_filter() {
                self.status_page.set_title(&bookie_app_services::tr!("No matches"));
                self.status_page
                    .set_description(Some(&bookie_app_services::tr!("Try a different search term or change the filters.")));
                self.status_page.set_icon_name(Some("system-search-symbolic"));
            } else {
                self.status_page.set_title(&bookie_app_services::tr!("No queries yet"));
                self.status_page.set_description(Some(&bookie_app_services::tr!(
                    "Run a query in the SQL editor and it will appear here."
                )));
                self.status_page.set_icon_name(Some("document-open-recent-symbolic"));
            }
            self.stack.set_visible_child_name("empty");
            self.refresh_selection_bar();
            return;
        }
        self.stack.set_visible_child_name("list");

        let (pinned, regular): (Vec<_>, Vec<_>) = entries.into_iter().partition(|e| e.pinned);
        let has_pinned = !pinned.is_empty();
        let has_regular = !regular.is_empty();
        self.pinned_group.set_visible(has_pinned);
        self.list_group.set_visible(has_regular);

        for entry in &pinned {
            let (row, popover, checkbox) = self.build_row(entry, sender.clone());
            self.pinned_listbox.append(&row);
            self.row_popovers.push(popover);
            self.row_checkboxes.push(checkbox);
        }
        for entry in &regular {
            let (row, popover, checkbox) = self.build_row(entry, sender.clone());
            self.listbox.append(&row);
            self.row_popovers.push(popover);
            self.row_checkboxes.push(checkbox);
        }

        self.refresh_selection_bar();
    }

    fn build_row(
        &self,
        entry: &Entry,
        sender: ComponentSender<Self>,
    ) -> (adw::ActionRow, gtk::PopoverMenu, gtk::CheckButton) {
        let id = entry.id;
        let row = row::row_shell(entry);
        let select_check = row::select_check(entry, self.select_mode, self.selected_ids.contains(&id), &sender);
        row.add_prefix(&select_check);
        row.add_suffix(&row::pin_button(entry, &sender));
        row.add_suffix(&row::delete_button(entry, &sender));

        let s_act = sender.clone();
        row.connect_activated(move |_| s_act.input(HistoryDialogInput::Activate(id)));

        let popover_menu = gtk::PopoverMenu::from_model(Some(&row::row_menu(entry)));
        popover_menu.set_has_arrow(true);
        popover_menu.set_parent(&row);
        row.insert_action_group("history-row", Some(&row::row_actions(entry, &row, &sender)));
        row::attach_context_menu(&row, &popover_menu);

        (row, popover_menu, select_check)
    }

    fn toggle_pin(&mut self, id: i64, sender: ComponentSender<Self>) {
        let pinned = !self.is_pinned(id);
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.pinned = pinned;
        }
        let history = self.history.clone();
        self.run_search_after(sender, async move {
            if let Err(e) = history.set_pinned(id, pinned).await {
                tracing::warn!(error = %e, "history set_pinned failed");
            }
        });
    }

    fn delete_one(&mut self, id: i64, sender: ComponentSender<Self>) {
        self.selected_ids.remove(&id);
        let history = self.history.clone();
        self.run_search_after(sender, async move {
            if let Err(e) = history.delete(id).await {
                tracing::warn!(error = %e, "history delete failed");
            }
        });
    }

    fn set_select_mode(&mut self, on: bool) {
        self.select_mode = on;
        if !on {
            self.selected_ids.clear();
            for cb in &self.row_checkboxes {
                cb.set_active(false);
            }
        }
        for cb in &self.row_checkboxes {
            cb.set_visible(on);
        }
        self.refresh_selection_bar();
    }

    fn confirm_delete_selected(&self, sender: ComponentSender<Self>) {
        // Bulk deletion is irreversible and can wipe pinned
        // entries the user marked as important; gate it
        // behind an AdwAlertDialog confirmation before
        // touching the database. Mirrors the Clear-All
        // pattern below.
        let n = self.selected_ids.len();
        if n == 0 {
            return;
        }
        let dialog = adw::AlertDialog::new(None, None);
        dialog.set_heading(Some(&if n == 1 {
            bookie_app_services::tr!("Delete this query?")
        } else {
            bookie_app_services::tr!("Delete {n} queries?").replace("{n}", &n.to_string())
        }));
        dialog.set_body(&bookie_app_services::tr!(
            "Selected entries will be permanently removed from history, including any pinned ones."
        ));
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("delete", &bookie_app_services::tr!("Delete"));
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let s = sender;
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                s.input(HistoryDialogInput::DeleteSelectedConfirmed);
            }
        });
        dialog.present(Some(&self.root));
    }

    fn delete_selected(&mut self, sender: ComponentSender<Self>) {
        let ids: Vec<i64> = self.selected_ids.drain().collect();
        if ids.is_empty() {
            return;
        }
        let history = self.history.clone();
        self.run_search_after(sender, async move {
            if let Err(e) = history.delete_many(&ids).await {
                tracing::warn!(error = %e, "history delete_many failed");
            }
        });
    }

    fn confirm_clear_all(&self, sender: ComponentSender<Self>) {
        let dialog = adw::AlertDialog::new(
            Some(&bookie_app_services::tr!("Clear all query history?")),
            Some(&bookie_app_services::tr!(
                "This permanently deletes every saved query, including pinned ones."
            )),
        );
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("clear", &bookie_app_services::tr!("Clear"));
        dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let s = sender;
        dialog.connect_response(None, move |_, response| {
            if response == "clear" {
                s.input(HistoryDialogInput::ClearAllConfirmed);
            }
        });
        dialog.present(Some(&self.root));
    }

    fn clear_all(&self, sender: ComponentSender<Self>) {
        let s = sender.clone();
        let history = self.history.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let _ = s;
                    if let Err(e) = history.clear_all().await {
                        tracing::warn!(error = %e, "history clear_all failed");
                    }
                    out.send(HistoryDialogCmd::Cleared).ok()
                })
                .drop_on_shutdown()
        });
    }

    fn open_storage_location(&self) {
        let path = self.history.path();
        let parent = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| path.to_path_buf());
        let file = gio::File::for_path(&parent);
        let launcher = gtk::FileLauncher::new(Some(&file));
        launcher.launch(
            Some(
                &self
                    .root
                    .root()
                    .and_then(|r| r.downcast::<gtk::Window>().ok())
                    .unwrap_or_default(),
            ),
            gio::Cancellable::NONE,
            |_| {},
        );
    }

    fn refresh_selection_bar(&self) {
        let n = self.selected_ids.len();
        if n == 0 {
            self.selection_bar.set_reveal_child(false);
            return;
        }
        self.selection_label
            .set_label(&bookie_app_services::trn!("{n} selected", "{n} selected", n).replace("{n}", &n.to_string()));
        self.selection_bar.set_reveal_child(true);
    }

    fn export_selected(&self, kind: &'static str, sender: ComponentSender<Self>) {
        let ids: Vec<i64> = self.selected_ids.iter().copied().collect();
        if ids.is_empty() {
            return;
        }
        let suggested = if kind == "sql" {
            "bookie-history.sql".to_string()
        } else {
            "bookie-history.csv".to_string()
        };
        let history = self.history.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let result = match kind {
                        "sql" => history.export_sql(&ids).await,
                        _ => history.export_csv(&ids).await,
                    };
                    let msg = match result {
                        Ok(text) => HistoryDialogCmd::ExportReady(text, suggested),
                        Err(e) => HistoryDialogCmd::ExportFailed(e.to_string()),
                    };
                    out.send(msg).ok()
                })
                .drop_on_shutdown()
        });
    }

    fn query_for_id(&self, id: i64) -> Option<String> {
        self.entries.iter().find(|e| e.id == id).map(|e| e.query.clone())
    }

    fn is_pinned(&self, id: i64) -> bool {
        self.entries.iter().any(|e| e.id == id && e.pinned)
    }
}

fn fts5_query(input: &str) -> String {
    // Tokenise on whitespace and quote each token. FTS5 then ANDs them by
    // default ("SELECT users" → match rows containing both terms anywhere),
    // matching what users expect from a search box. Quoting protects each
    // token from FTS5's own operator chars (parens, AND, OR, NEAR, *).
    input
        .split_whitespace()
        .map(|tok| {
            let escaped = tok.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn preview_title(query: &str) -> String {
    let first_line = query
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim().starts_with("--"))
        .unwrap_or("")
        .trim();
    let collected: String = first_line.chars().take(80).collect();
    if collected.chars().count() < first_line.chars().count() {
        format!("{collected}…")
    } else if collected.is_empty() {
        bookie_app_services::tr!("Empty query")
    } else {
        collected
    }
}

fn format_subtitle(entry: &Entry) -> String {
    let when = format_relative_time(entry.executed_at);
    let mut parts: Vec<String> = vec![entry.connection_name.clone(), entry.driver_id.clone(), when];
    if let Some(d) = entry.duration_ms {
        parts.push(format!("{d} ms"));
    }
    if entry.cancelled {
        parts.push(bookie_app_services::tr!("cancelled"));
    } else if let Some(err) = &entry.error {
        let trimmed: String = err.chars().take(48).collect();
        parts.push(if trimmed.chars().count() < err.chars().count() {
            format!("{trimmed}…")
        } else {
            trimmed
        });
    } else if let Some(rows) = entry.rows_affected {
        parts.push(bookie_app_services::tr!("{n} row(s)").replace("{n}", &rows.to_string()));
    }
    parts.join(" · ")
}

fn format_relative_time(when: SystemTime) -> String {
    let now = SystemTime::now();
    let diff = now.duration_since(when).unwrap_or_default();
    let secs = diff.as_secs();
    if secs < 60 {
        bookie_app_services::tr!("just now")
    } else if secs < 3600 {
        let n = secs / 60;
        bookie_app_services::tr!("{n} min ago").replace("{n}", &n.to_string())
    } else if secs < 86_400 {
        let n = secs / 3600;
        bookie_app_services::tr!("{n} h ago").replace("{n}", &n.to_string())
    } else if secs < 30 * 86_400 {
        let n = secs / 86_400;
        bookie_app_services::tr!("{n} d ago").replace("{n}", &n.to_string())
    } else {
        let dt = chrono::DateTime::<chrono::Local>::from(when);
        dt.format("%Y-%m-%d").to_string()
    }
}

fn clear_listbox(listbox: &gtk::ListBox) {
    while let Some(child) = listbox.first_child() {
        listbox.remove(&child);
    }
}

fn source_for_index(index: u32) -> Option<Source> {
    match index {
        1 => Some(Source::Editor),
        2 => Some(Source::Structure),
        3 => Some(Source::Browse),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_ran_from_choice_keeps_every_source() {
        assert_eq!(source_for_index(0), None);
    }

    #[test]
    fn each_ran_from_choice_selects_its_own_source() {
        assert_eq!(source_for_index(1), Some(Source::Editor));
        assert_eq!(source_for_index(2), Some(Source::Structure));
        assert_eq!(source_for_index(3), Some(Source::Browse));
    }

    #[test]
    fn an_index_beyond_the_ran_from_list_keeps_every_source() {
        assert_eq!(source_for_index(9), None);
    }
}

#[cfg(test)]
mod export_tests;

#[cfg(test)]
mod lifetime_tests;

mod build;
mod row;
mod export;

#[cfg(test)]
mod confirmation_tests;
