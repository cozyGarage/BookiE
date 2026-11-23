use super::*;
use bookie_app_services::structure_change_plan::StructureChangeKind;
use crate::palette_state::PaletteMode;

fn is_edit_palette_action(id: &str) -> bool {
    matches!(
        id,
        "save-changes"
            | "undo-edit"
            | "redo-edit"
            | "delete-rows"
            | "insert-row"
            | "import-csv"
            | "add-column"
            | "rename-column"
            | "drop-column"
            | "add-index"
            | "drop-index"
            | "show-structure"
            | "go-to-reference"
            | "show-referencing"
            | "show-create-table"
            | "empty-table"
            | "drop-table"
            | "resolve-outcome"
            | "add-foreign-key"
            | "rename-table"
            | "drop-foreign-key"
    )
}

impl GridView {
    pub(crate) fn palette_actions(&self) -> Vec<Action> {
        if self.palette.fonts {
            return self
                .fonts
                .names
                .iter()
                .map(|name| Action {
                    id: "font-choice",
                    label: name.clone(),
                    enabled: true,
                })
                .collect();
        }
        let check = self.palette.address.as_ref().map(|address| Action {
            id: "check-tcp",
            label: format!("Check TCP port {address}"),
            enabled: true,
        });
        let listen = self.palette.pattern.as_ref().map(|pattern| Action {
            id: "sample-pubsub",
            label: match self.live.as_ref().map(|live| live.driver_id.as_str()) {
                Some("postgres") => format!("Listen to PostgreSQL notifications on {pattern}"),
                _ => format!("Listen to Redis channels {pattern}"),
            },
            enabled: true,
        });
        let follow = self.palette.follow.as_ref().map(|channel| Action {
            id: "follow-pubsub",
            label: match self.live.as_ref().map(|live| live.driver_id.as_str()) {
                Some("redis") => format!("Follow Redis channels {channel} until stopped"),
                _ => format!("Follow PostgreSQL notifications on {channel} until stopped"),
            },
            enabled: self.panel.follow.is_none(),
        });
        let stop = self.panel.follow.as_ref().map(|state| Action {
            id: "stop-follow",
            label: format!("Stop following {}", state.channel),
            enabled: true,
        });
        check
            .into_iter()
            .chain(listen)
            .chain(follow)
            .chain(stop)
            .chain(self.csv_delimiter_actions())
            .chain(self.export_limit_actions())
            .chain(self.grid_actions())
            .chain(self.activity_actions())
            .chain(self.health_actions())
            .chain(self.value_search_actions())
            .chain(self.schema_diagram_actions())
            .chain(self.run_cursor_actions())
            .chain(self.paste_actions())
            .chain(self.filter_rule_actions())
            .chain(self.server_filter_actions())
            .chain(self.diagram_actions())
            .chain(self.update_actions())
            .chain(self.backup_actions())
            .chain(self.refresh_actions())
            .chain(self.reconnect_actions())
            .chain(self.new_window_actions())
            .chain(self.disconnect_actions())
            .chain(self.sql_file_actions())
            .chain(self.closed_tab_actions())
            .chain(self.recent_tab_actions())
            .chain(self.tables_export_actions())
            .chain(self.chrome_actions())
            .collect()
    }

    fn export_limit_actions(&self) -> Vec<Action> {
        let typed = self.palette.limit.as_ref().map(|(text, parsed)| Action {
            id: "set-export-limit",
            label: match parsed {
                Ok(_) => format!("Export limit {text}: stop CSV export after {text} rows"),
                Err(()) => format!("Export limit {text}: {}", bookie_gpui::export_limit::error_label()),
            },
            enabled: true,
        });
        let reset = self.export.limit.map(|limit| Action {
            id: "reset-export-limit",
            label: format!("Export limit: remove (now {limit} rows)"),
            enabled: true,
        });
        typed.into_iter().chain(reset).collect()
    }

    fn csv_delimiter_actions(&self) -> Vec<Action> {
        let typed = self.palette.delimiter.as_ref().map(|(text, parsed)| Action {
            id: "set-csv-delimiter",
            label: match parsed {
                Ok(_) => format!("CSV delimiter {text}: use it for CSV export and copy"),
                Err(error) => format!(
                    "CSV delimiter {text}: {}",
                    bookie_gpui::csv_delimiter::error_label(*error)
                ),
            },
            enabled: true,
        });
        let reset = self.export.csv_delimiter.as_ref().map(|delimiter| Action {
            id: "reset-csv-delimiter",
            label: format!("CSV delimiter: back to the comma (now {})", delimiter.as_str()),
            enabled: true,
        });
        typed.into_iter().chain(reset).collect()
    }

    fn grid_actions(&self) -> Vec<Action> {
        let mut actions = self.grid_nav_actions();
        actions.extend(self.grid_selection_actions());
        actions
    }

    fn grid_nav_actions(&self) -> Vec<Action> {
        let filtering = matches!(self.mode, Mode::Filtered { .. });
        let paged = !self.continuous;
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        vec![
            action("first", "Go to first page", paged),
            action("previous", "Go to previous page", paged && self.pager.page > 0),
            action(
                "next",
                "Go to next page",
                paged && self.pager.page + 1 < self.pager.pages(),
            ),
            action("last", "Go to last page", paged),
            action("find", "Find in loaded rows", !filtering),
            action("filter", "Filter loaded rows", true),
            action("clear-filter", "Clear filter", filtering),
            action("compare", "Compare results", true),
            action(
                "detail",
                if self.detail_open {
                    "Hide cell details"
                } else {
                    "Show cell details"
                },
                true,
            ),
            action(
                "show-row-vertically",
                if self.row_detail_open {
                    "Hide row details"
                } else {
                    "Show this row vertically"
                },
                self.cell_cursor.is_some() && self.compare.is_none(),
            ),
            action(
                "count-table-rows",
                "Count rows in database table",
                self.live.as_ref().is_some_and(|live| {
                    live.supports_table_row_count() && !self.grid_is_query && self.compare.is_none()
                }),
            ),
            action(
                "continuous",
                if self.continuous {
                    "Switch to paged scrolling"
                } else {
                    "Switch to continuous scrolling"
                },
                true,
            ),
        ]
    }

    fn grid_selection_actions(&self) -> Vec<Action> {
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        vec![
            action("copy", "Copy selected rows as TSV", self.selection.is_some()),
            action("copy-csv", "Copy selected rows as CSV", self.selection.is_some()),
            action("set-cells", "Set selected cells to a value…", self.set_cells_enabled()),
            action("selection-stats", "Show statistics for the selected rows", self.selection_stats_enabled()),
            action("export-file", "Export to a file…", self.export_file_enabled()),
            action("export-csv", "Export current page or selection as CSV", true),
            action("copy-json", "Copy selected rows as JSON", self.selection.is_some()),
            action(
                "copy-markdown",
                "Copy selected rows as a Markdown table",
                self.selection.is_some(),
            ),
            action(
                "copy-in-list",
                "Copy the active column as an IN list",
                self.selection.is_some(),
            ),
            action(
                "copy-insert",
                "Copy selected rows as INSERT statements",
                self.selection.is_some() && !self.grid_is_query && !self.catalog.tables.is_empty(),
            ),
            action("clear-selection", "Clear row selection", self.selection.is_some()),
            action("edit", "Edit the active cell", self.cell_cursor.is_some()),
            action(
                "discard-view-edits",
                "Discard all view-only edits",
                self.has_view_edits(),
            ),
        ]
    }

    fn activity_actions(&self) -> Vec<Action> {
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        let supported = self
            .live
            .as_ref()
            .map(|live| bookie_core::activity_kinds(&live.driver_id))
            .unwrap_or_default();
        let has = |kind| supported.contains(&kind);
        vec![
            action(
                "sessions",
                if self.live.as_ref().is_some_and(|l| l.driver_id == "postgres") {
                    "Show PostgreSQL sessions and blockers"
                } else {
                    "Show active database sessions"
                },
                has(bookie_core::ActivityQuery::Sessions),
            ),
            action(
                "blocking-locks",
                "Show blocking locks",
                has(bookie_core::ActivityQuery::BlockingLocks),
            ),
            action(
                "long-running",
                "Show long-running queries",
                has(bookie_core::ActivityQuery::LongRunning),
            ),
            action(
                "replication-lag",
                "Show replication status and lag",
                has(bookie_core::ActivityQuery::ReplicationLag),
            ),
            action(
                "server-variables",
                "Show server variables and configuration",
                has(bookie_core::ActivityQuery::ServerVariables),
            ),
            action(
                "server-status",
                "Show server runtime status",
                has(bookie_core::ActivityQuery::ServerStatus),
            ),
            action("status", "Show connection and tunnel status", self.live.is_some()),
        ]
    }

    fn chrome_actions(&self) -> Vec<Action> {
        let mut actions = self.picker_actions();
        actions.extend(self.structure_actions());
        actions.extend(self.workspace_actions());
        actions.extend(self.filter_actions());
        actions.extend(self.style_actions());
        actions
    }

    fn picker_actions(&self) -> Vec<Action> {
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        vec![
            action("quick-switcher", "Go to table", !self.catalog.tables.is_empty()),
            action("jump-column", "Jump to column", self.tabs.shows_grid_for(&self.catalog.grid_table)),
            action("shortcuts", "Keyboard shortcuts", true),
            action(
                "save-favorite",
                "Save query as favorite",
                matches!(self.tabs.current(), Tab::Query),
            ),
            action("favorites", "Open a favorite", true),
            action("undo-edit", "Undo edit", self.can_undo_edit()),
            action("redo-edit", "Redo edit", self.can_redo_edit()),
            action(
                "save-changes",
                "Save changes",
                self.can_save_edits() && !self.cell_edit.edits.is_empty(),
            ),
            action(
                "import-csv",
                "Import CSV file…",
                self.can_save_edits() && self.cell_edit.edits.is_empty() && self.compare.is_none() && !self.form_open(),
            ),
            action("resolve-outcome", "Resolve unknown write outcome…", self.resolve_enabled()),
            action(
                "insert-row",
                "Add a row",
                self.can_save_edits() && self.cell_edit.edits.is_empty() && self.compare.is_none() && !self.form_open(),
            ),
            action(
                "delete-rows",
                "Delete selected rows",
                self.can_save_edits()
                    && (self.selection.is_some() || self.cell_cursor.is_some())
                    && self.cell_edit.edits.is_empty()
                    && self.compare.is_none(),
            ),
            action("delete-favorite", "Delete a favorite", true),
            action(
                "reveal-sidebar",
                "Reveal table in sidebar",
                matches!(self.tabs.current(), Tab::Table(_)),
            ),
        ]
    }

    fn filter_actions(&self) -> Vec<Action> {
        let enabled = self.saved_filter_enabled();
        let action = |id, label: &str| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        vec![
            action("save-filter", "Save the current filter…"),
            action("saved-filters", "Apply a saved filter"),
            action("delete-saved-filter", "Delete a saved filter"),
        ]
    }

    fn structure_actions(&self) -> Vec<Action> {
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        vec![
            action("show-create-table", "Show CREATE TABLE", self.structure_info_enabled()),
            action("empty-table", "Empty the table…", self.structure_change_enabled()),
            action("drop-table", "Drop the table…", self.structure_change_enabled()),
            action("go-to-reference", "Go to the referenced row", self.fk_nav_enabled()),
            action("show-referencing", "Show referencing rows", self.fk_nav_enabled()),
            action("show-structure", "Show table structure", self.structure_info_enabled()),
            action("add-column", "Add a column…", self.structure_change_enabled()),
            action("rename-column", "Rename a column…", self.structure_change_enabled()),
            action("drop-column", "Drop a column…", self.structure_change_enabled()),
            action("add-index", "Create an index…", self.structure_change_enabled()),
            action("drop-index", "Drop an index…", self.structure_change_enabled()),
            action("rename-table", "Rename the table…", self.structure_change_enabled()),
            action("add-foreign-key", "Add a foreign key…", self.foreign_key_change_enabled()),
            action("drop-foreign-key", "Drop a foreign key…", self.foreign_key_change_enabled()),
        ]
    }

    fn workspace_actions(&self) -> Vec<Action> {
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        vec![
            action("query", "New query tab", true),
            action("run", "Run the query", matches!(self.tabs.current(), Tab::Query)),
            action(
                "explain",
                "Explain the query",
                matches!(self.tabs.current(), Tab::Query) && self.live.is_some(),
            ),
            action("data-dictionary", "Show the data dictionary", self.live.is_some()),
            action(
                "previous-run",
                "Previous result",
                matches!(self.tabs.current(), Tab::Query) && !self.run_tabs.slots.is_empty() && self.run_tabs.active > 0,
            ),
            action(
                "next-run",
                "Next result",
                matches!(self.tabs.current(), Tab::Query) && self.run_tabs.active + 1 < self.run_tabs.slots.len(),
            ),
            action(
                "close-run",
                "Close current result",
                matches!(self.tabs.current(), Tab::Query) && !self.run_tabs.slots.is_empty(),
            ),
            action(
                "results",
                if self.panel.open {
                    "Hide the results pane"
                } else {
                    "Show the results pane"
                },
                matches!(self.tabs.current(), Tab::Query),
            ),
            action("home", "Open Home", true),
            action("new-connection", "New connection", true),
            action("history", "Open history", true),
            action("audit", "Open audit log", true),
            action("settings", "Open settings", true),
            action(
                "table",
                &format!("Open {}", table_label(&self.catalog.grid_table)),
                !self.catalog.tables.is_empty(),
            ),
        ]
    }

    fn style_actions(&self) -> Vec<Action> {
        let action = |id, label: &str, enabled| Action {
            id,
            label: label.to_string(),
            enabled,
        };
        vec![
            action(
                "sidebar",
                if self.sidebar.open {
                    "Hide the sidebar"
                } else {
                    "Show the sidebar"
                },
                true,
            ),
            action("font", "Choose font…", true),
            action("select-theme", "Choose colour theme…", true),
            action(
                "theme",
                if self.dark_mode {
                    "Switch to light theme"
                } else {
                    "Switch to dark theme"
                },
                true,
            ),
        ]
    }

    pub(crate) fn palette_shown(&self, cx: &App) -> Vec<usize> {
        palette_matches(self.palette.input.read(cx).content(), &self.palette_actions())
    }

    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.open = true;
        self.palette.fonts = false;
        self.palette.mode = PaletteMode::Commands;
        self.palette
            .input
            .update(cx, |input, cx| input.set_placeholder(PaletteMode::Commands.placeholder(), cx));
        self.palette.index = 0;
        self.palette.input.update(cx, |input, cx| input.set_content("", cx));
        window.focus(&self.palette.input.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.open = false;
        self.palette.fonts = false;
        self.palette.mode = PaletteMode::Commands;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn run_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.mode != PaletteMode::Commands {
            return self.run_picker(window, cx);
        }
        let actions = self.palette_actions();
        let shown = palette_matches(self.palette.input.read(cx).content(), &actions);
        let Some(index) = shown
            .get(self.palette.index.min(shown.len().saturating_sub(1)))
            .copied()
        else {
            return;
        };
        let id = actions[index].id;
        if id == "font" {
            self.palette.fonts = true;
            self.palette.index = 0;
            self.palette.input.update(cx, |input, cx| input.set_content("", cx));
            cx.notify();
            return;
        }
        if id == "font-choice" {
            self.fonts.current = actions[index].label.clone().into();
            self.close_palette(window, cx);
            return;
        }
        self.close_palette(window, cx);
        self.execute_palette_action(id, window, cx);
        cx.notify();
    }

    pub(crate) fn run_palette_id(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.execute_palette_action(id, window, cx);
    }

    fn execute_palette_action(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        if self.execute_grid_palette_action(id, window, cx) {
            return;
        }
        if self.execute_activity_palette_action(id, window, cx) {
            return;
        }
        self.execute_workspace_palette_action(id, window, cx);
    }

    fn execute_grid_palette_action(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match id {
            "first" => self.pager.first(),
            "previous" => self.pager.previous(),
            "next" => self.pager.next(),
            "last" => self.pager.last(),
            "find" => window.focus(&self.search.find.read(cx).focus_handle(cx), cx),
            "filter" => {
                self.search.filter_visible = true;
                window.focus(&self.search.filter.read(cx).focus_handle(cx), cx);
            }
            "clear-filter" => self.clear_filter(),
            "compare" => self.compare(false, cx),
            "detail" => self.toggle_detail(),
            "show-row-vertically" => self.toggle_row_detail(),
            "count-table-rows" => self.count_table_rows(cx),
            "continuous" => {
                self.continuous = !self.continuous;
                self.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
            }
            "copy" => self.copy_selection(false, cx),
            "copy-csv" => self.copy_selection_as(CopyFormat::Csv, cx),
            "export-csv" => self.export_selection_or_page(cx),
            "export-file" => self.open_export_file_bar(cx),
            "set-cells" => self.open_set_cells_bar(cx),
            "selection-stats" => self.show_selection_stats(cx),
            "set-csv-delimiter" => self.set_csv_delimiter(),
            "reset-csv-delimiter" => self.set_csv_delimiter_to(None),
            "set-export-limit" => self.set_export_limit(),
            "reset-export-limit" => self.set_export_limit_to(None),
            "copy-json" => self.copy_selection_as(CopyFormat::Json, cx),
            "copy-markdown" => self.copy_selection_as(CopyFormat::Markdown, cx),
            "copy-in-list" => self.copy_selection_as(CopyFormat::InList, cx),
            "copy-insert" => self.copy_selection_as(CopyFormat::Insert, cx),
            "discard-view-edits" => self.discard_view_edits(),
            "clear-selection" => self.selection = None,
            "edit" => self.grid_key("enter", false, false, window, cx),
            _ => return false,
        }
        true
    }

    fn execute_activity_palette_action(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match id {
            "sessions" => self.show_sessions(window, cx),
            "blocking-locks" => self.show_activity(bookie_core::ActivityQuery::BlockingLocks, window, cx),
            "long-running" => self.show_activity(bookie_core::ActivityQuery::LongRunning, window, cx),
            "replication-lag" => self.show_activity(bookie_core::ActivityQuery::ReplicationLag, window, cx),
            "server-variables" => self.show_activity(bookie_core::ActivityQuery::ServerVariables, window, cx),
            "server-status" => self.show_activity(bookie_core::ActivityQuery::ServerStatus, window, cx),
            "kafka-topics" => {
                if let Some(broker) = self.palette.address.clone() {
                    self.kafka_topics(broker, cx);
                }
            }
            "status" => self.show_status(),
            "follow-pubsub" => {
                if let Some(channel) = self.palette.follow.clone() {
                    self.start_follow(channel, cx);
                }
            }
            "stop-follow" => self.stop_follow(cx),
            "sample-pubsub" => {
                if let Some(pattern) = self.palette.pattern.clone() {
                    self.sample_pubsub(pattern, cx);
                }
            }
            "check-tcp" => {
                if let Some(address) = self.palette.address.clone() {
                    self.check_tcp(address, cx);
                }
            }
            _ => return false,
        }
        true
    }

    fn execute_edit_palette_action(&mut self, id: &'static str, cx: &mut Context<Self>) {
        match id {
            "save-changes" => self.request_save(cx),
            "undo-edit" => self.undo_edit(cx),
            "redo-edit" => self.redo_edit(cx),
            "delete-rows" => self.request_delete(cx),
            "import-csv" => self.open_import_bar(cx),
            "show-structure" => self.show_structure(cx),
            "go-to-reference" => self.go_to_referenced_row(cx),
            "show-referencing" => self.show_referencing_rows(cx),
            "show-create-table" => self.show_create_table(cx),
            "empty-table" => self.open_structure_bar(StructureChangeKind::TruncateTable, cx),
            "drop-table" => self.open_structure_bar(StructureChangeKind::DropTable, cx),
            "resolve-outcome" => self.open_resolve_panel(cx),
            "add-column" => self.open_structure_bar(StructureChangeKind::AddColumn, cx),
            "rename-column" => self.open_structure_bar(StructureChangeKind::RenameColumn, cx),
            "drop-column" => self.open_structure_bar(StructureChangeKind::DropColumn, cx),
            "add-index" => self.open_structure_bar(StructureChangeKind::AddIndex, cx),
            "drop-index" => self.open_structure_bar(StructureChangeKind::DropIndex, cx),
            "rename-table" => self.open_structure_bar(StructureChangeKind::RenameTable, cx),
            "add-foreign-key" => self.open_structure_bar(StructureChangeKind::AddForeignKey, cx),
            "drop-foreign-key" => self.open_structure_bar(StructureChangeKind::DropForeignKey, cx),
            _ => self.open_insert_form(cx),
        }
    }

    fn execute_workspace_palette_action(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            "sidebar" => self.sidebar.open = !self.sidebar.open,
            "quick-switcher" => self.open_picker(PaletteMode::Tables, window, cx),
            "jump-column" => self.open_picker(PaletteMode::Columns, window, cx),
            "shortcuts" => self.open_picker(PaletteMode::Shortcuts, window, cx),
            "save-favorite" => self.save_favorite(cx),
            "favorites" => self.open_picker(PaletteMode::Favorites, window, cx),
            "save-filter" => self.open_picker(PaletteMode::NameFilter, window, cx),
            "select-theme" => self.open_picker(PaletteMode::Themes, window, cx),
            "saved-filters" => self.open_picker(PaletteMode::SavedFilters, window, cx),
            "delete-saved-filter" => self.open_picker(PaletteMode::DeleteSavedFilter, window, cx),
            _ if is_edit_palette_action(id) => self.execute_edit_palette_action(id, cx),
            "delete-favorite" => self.open_picker(PaletteMode::DeleteFavorite, window, cx),
            "reveal-sidebar" => self.reveal_in_sidebar(cx),
            "new-connection" => {
                self.connection_form.editing = None;
                self.tabs.open_new_connection();
                self.sync_grid_owner();
            }
            "audit" => {
                self.tabs.open_audit_log();
                self.reload_audit_page(cx);
                self.sync_grid_owner();
            }
            "history" => {
                self.tabs.open_history();
                self.reload_history_page(cx);
                self.sync_grid_owner();
            }
            "settings" => {
                self.tabs.open_settings();
                self.sync_grid_owner();
            }
            "home" => {
                self.tabs.open_home();
                self.reload_history(cx);
                self.sync_grid_owner();
            }
            "query" => {
                self.tabs.open_query();
                self.sync_grid_owner();
                window.focus(&self.focus, cx);
            }
            "run" => self.run_query(cx),
            "explain" => self.explain_query(cx),
            "data-dictionary" => self.show_data_dictionary(cx),
            _ if self.execute_tool_palette_action(id, window, cx) => {}
            "previous-run" => self.activate_run(self.run_tabs.active - 1),
            "next-run" => self.activate_run(self.run_tabs.active + 1),
            "close-run" => self.close_run(self.run_tabs.active),
            "results" => self.toggle_results(),
            "table" => {
                let name = self.catalog.grid_table.clone();
                self.open_table_named(&name, window, cx);
            }
            "theme" => {
                self.dark_mode = !self.dark_mode;
                Theme::current(cx).toggled().install(cx);
                window.refresh();
            }
            _ => {}
        }
    }
}
