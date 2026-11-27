use bookie_gpui::font_settings;
use bookie_gpui::keymap::{Chord, Command};
use bookie_results::grid_cursor::{Cursor, move_cursor};
use gpui::{App, Context, Focusable, KeyDownEvent, KeystrokeEvent, ScrollStrategy, Window};

use crate::palette_state::PaletteMode;
use crate::{GridView, Tab, palette_step};

impl GridView {
    pub(crate) fn intercept_key(&mut self, event: &KeystrokeEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let top = self.top_surface();
        if top.is_some_and(Surface::blocks_keys) {
            self.surface_key(key, cx);
            cx.stop_propagation();
            return;
        }
        if self.record_shortcut_key(&event.keystroke, cx) {
            cx.stop_propagation();
            return;
        }
        if let Some(surface) = top {
            if self.surface_key(key, cx) {
                cx.stop_propagation();
            }
            if surface != Surface::Notifications || key == "escape" {
                return;
            }
        }
        if self.palette.open {
            if palette_navigation_key(key, event.keystroke.modifiers) {
                self.move_palette_selection(key, cx);
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        if key == "escape" && self.can_stop_query() && self.sql_input.read(cx).focus_handle(cx).is_focused(window) {
            self.stop_query();
            cx.stop_propagation();
            cx.notify();
            return;
        }
        self.intercept_grid_key(event, window, cx);
    }

    fn intercept_grid_key(&mut self, event: &KeystrokeEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        let control = control_modifier(modifiers);
        if !self.grid_key_ready(window, control || modifiers.alt) {
            return;
        }
        let key = event.keystroke.key.as_str();
        if modifiers.shift && matches!(key, "up" | "down") {
            self.extend_selection_with_arrow(key, cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let plain = !(modifiers.shift || control || modifiers.alt);
        if plain && self.cross_page_edge(key) {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn surface_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        match key {
            "escape" => self.close_top_surface(cx),
            "enter" => {
                let submitted = self.submit_top_surface(cx);
                cx.notify();
                submitted
            }
            _ => false,
        }
    }

    pub(crate) fn top_surface(&self) -> Option<Surface> {
        Surface::ORDER
            .iter()
            .copied()
            .find(|surface| self.surface_open(*surface))
    }

    fn surface_open(&self, surface: Surface) -> bool {
        match surface {
            Surface::Approval => self.approval.current.is_some(),
            Surface::HostKey => self.host_key.current.is_some(),
            Surface::CloseConfirm => self.close_confirm,
            Surface::CancelProcess => self.cancel_confirm.is_some(),
            Surface::DeleteConnection => self.connection_form.open && self.connection_form.confirm_delete,
            Surface::Notifications => self.notices.open,
            Surface::ConnectionForm => self.connection_form.open,
            Surface::SaveReview => self.save.pending.is_some(),
            Surface::Import => self.import.open,
            Surface::ExportFile => self.export.file.open,
            Surface::SchemaDiagram => self.search.diagram.open,
            Surface::Insert => self.insert.open,
            Surface::DataCompare => self.search.data_compare.open,
            Surface::Structure => self.structure.open,
            Surface::SetCells => self.cell_edit.set.open,
            Surface::Resolve => self.resolve.open,
            Surface::Backup => self.search.backup.open,
            Surface::SqlFile => self.search.sql_file.open,
            Surface::TablesExport => self.search.tables_export.open,
            Surface::ConnectionBundle => {
                self.connection_bundle.pending.is_some() || self.connection_bundle.encrypted.is_some()
            }
        }
    }

    pub(crate) fn close_top_surface(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(surface) = self.top_surface() else {
            return false;
        };
        self.close_surface(surface, cx);
        cx.notify();
        true
    }

    pub(crate) fn submit_top_surface(&mut self, cx: &mut Context<Self>) -> bool {
        self.top_surface()
            .is_some_and(|surface| self.submit_surface(surface, cx))
    }

    fn close_surface(&mut self, surface: Surface, cx: &mut Context<Self>) {
        match surface {
            Surface::Approval => self.answer_approval(bookie_app_services::approval_prompt::ApprovalOutcome::Deny, cx),
            Surface::HostKey => self.answer_host_key(false, cx),
            Surface::CloseConfirm => self.keep_editing(cx),
            Surface::CancelProcess => self.keep_process_running(cx),
            Surface::DeleteConnection => self.cancel_delete_connection(cx),
            Surface::Notifications => self.close_notifications(cx),
            Surface::ConnectionForm => self.close_connection_dialog(cx),
            Surface::SaveReview => self.cancel_save(cx),
            Surface::Import => self.cancel_import_bar(cx),
            Surface::ExportFile => self.close_export_file_bar(),
            Surface::SchemaDiagram => self.close_schema_diagram_export(),
            Surface::Insert => self.cancel_insert_form(cx),
            Surface::DataCompare => self.close_data_compare(cx),
            Surface::Structure => self.cancel_structure_bar(cx),
            Surface::SetCells => self.close_set_cells_bar(),
            Surface::Resolve => self.close_resolve_panel(),
            Surface::Backup => self.search.backup.open = false,
            Surface::SqlFile => self.search.sql_file.open = false,
            Surface::TablesExport => self.search.tables_export.open = false,
            Surface::ConnectionBundle => self.cancel_connection_import(cx),
        }
    }

    fn submit_surface(&mut self, surface: Surface, cx: &mut Context<Self>) -> bool {
        match surface {
            Surface::SaveReview if self.save.pending.as_ref().is_some_and(|pending| !pending.destructive) => {
                self.confirm_save(cx)
            }
            Surface::Import => self.review_import(cx),
            Surface::ExportFile => self.write_export_file(cx),
            Surface::SchemaDiagram => self.write_schema_diagram(cx),
            Surface::Insert => self.review_insert(cx),
            Surface::Structure => self.review_structure(cx),
            Surface::SetCells => self.apply_set_cells(false, cx),
            Surface::Backup if !self.search.backup.restore => self.start_backup_job(cx),
            Surface::SqlFile => self.run_sql_file(cx),
            Surface::TablesExport => self.start_tables_export(cx),
            Surface::ConnectionBundle if self.connection_bundle.pending.is_none() => self.unlock_connection_import(cx),
            Surface::Notifications | Surface::ConnectionForm => return false,
            _ => {}
        }
        true
    }

    fn move_palette_selection(&mut self, key: &str, cx: &mut Context<Self>) {
        let count = self.palette_len(cx);
        self.palette.index = self.palette.index.min(count.saturating_sub(1));
        self.palette.index = match key {
            "home" => 0,
            "end" => count.saturating_sub(1),
            "pageup" => self.palette.index.saturating_sub(10),
            "pagedown" => self.palette.index.saturating_add(10).min(count.saturating_sub(1)),
            _ => palette_step(self.palette.index, count, key == "down"),
        };
        if count > 0 {
            self.palette
                .scroll
                .scroll_to_item(self.palette.index, ScrollStrategy::Nearest);
        }
    }

    fn grid_key_ready(&self, window: &Window, blocked: bool) -> bool {
        !blocked
            && self.focus.is_focused(window)
            && self.cell_edit.active.is_none()
            && self.compare.is_none()
            && !self.continuous
            && !self.palette.open
            && self.tabs.shows_grid_for(&self.catalog.grid_table)
    }

    fn extend_selection_with_arrow(&mut self, key: &str, cx: &App) {
        if let (None, Some(cursor)) = (self.selection, self.cell_cursor) {
            self.click_row(self.base() + cursor.row as u64, false);
        }
        if self.cross_page_edge(key) {
            if let Some(cursor) = self.cell_cursor {
                self.click_row(self.base() + cursor.row as u64, true);
            }
            return;
        }
        let projection = self.table.read(cx).delegate().clone_projection();
        let current = self.cell_cursor.map(|cursor| Cursor {
            row: cursor.row,
            column: projection.visible_index(cursor.column).unwrap_or(0),
        });
        let Some(mut next) = move_cursor(
            current,
            key,
            self.visible_count(),
            projection.len(),
            self.viewport_rows,
            false,
        ) else {
            return;
        };
        next.column = projection.source_index(next.column).unwrap_or(0);
        self.place_cursor(Some(next));
        self.click_row(self.base() + next.row as u64, true);
    }

    pub(crate) fn cross_page_edge(&mut self, key: &str) -> bool {
        let Some(cursor) = self.cell_cursor else { return false };
        let rows = self.visible_count();
        match key {
            "down" if cursor.row + 1 == rows && self.next_page() => {
                let row = if self.pager.append { rows } else { 0 };
                self.place_cursor(Some(Cursor {
                    row,
                    column: cursor.column,
                }));
                true
            }
            "up" if cursor.row == 0 && self.previous_page() => {
                let row = self.visible_count().saturating_sub(1);
                self.place_cursor(Some(Cursor {
                    row,
                    column: cursor.column,
                }));
                true
            }
            _ => false,
        }
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.top_surface()
            .is_some_and(|surface| surface != Surface::Notifications)
            || self.cell_edit.active.as_ref().is_some_and(|editing| editing.full_value)
    }

    pub(crate) fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.approval.current.is_some() || self.save.pending.is_some() {
            cx.stop_propagation();
            return;
        }
        if self.modal_open() {
            return;
        }
        let stroke = &event.keystroke;
        let modifiers = stroke.modifiers;
        let control = control_modifier(modifiers);
        let key = stroke.key.as_str();
        if Chord::new(control, modifiers.shift, modifiers.alt, key) == bookie_gpui::keymap::last_edit_chord() {
            self.go_to_last_edit(window, cx);
            cx.notify();
            return;
        }
        if self.palette.open {
            match key {
                key if palette_navigation_key(key, modifiers) => {
                    let count = self.palette_len(cx);
                    self.palette.index = self.palette.index.min(count.saturating_sub(1));
                    self.palette.index = match key {
                        "home" => 0,
                        "end" => count.saturating_sub(1),
                        "pageup" => self.palette.index.saturating_sub(10),
                        "pagedown" => self.palette.index.saturating_add(10).min(count.saturating_sub(1)),
                        _ => palette_step(self.palette.index, count, key == "down"),
                    };
                    if count > 0 {
                        self.palette
                            .scroll
                            .scroll_to_item(self.palette.index, ScrollStrategy::Nearest);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }
                "escape" => self.close_palette(window, cx),
                _ => {}
            }
            return;
        }
        if self.focus.is_focused(window)
            && !control
            && matches!(
                key,
                "up" | "down" | "left" | "right" | "home" | "end" | "pageup" | "pagedown"
            )
        {
            return;
        }
        if control && matches!(self.tabs.current(), Tab::Query) {
            if let Some(delta) = zoom_key(key) {
                self.zoom_editor(delta, cx);
                return;
            }
        }
        let chord = Chord::new(control, modifiers.shift, modifiers.alt, key);
        if let Some(command) = self.keymap.resolve(&chord) {
            self.run_command(command, window, cx);
            cx.notify();
            return;
        }
        if control && !matches!(key, "home" | "end") {
            cx.notify();
            return;
        }
        if self.focus.is_focused(window)
            && self.cell_edit.active.is_none()
            && self.compare.is_none()
            && self.tabs.shows_grid_for(&self.catalog.grid_table)
        {
            if chord == bookie_gpui::keymap::full_value_chord() {
                self.open_value_editor(window, cx);
            } else {
                self.grid_key(key, modifiers.shift, control, window, cx);
            }
        }
    }

    pub(crate) fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::Palette => self.open_palette(window, cx),
            Command::QuickSwitcher => self.open_picker(PaletteMode::Tables, window, cx),
            Command::JumpColumn => self.open_picker(PaletteMode::Columns, window, cx),
            Command::RevealInSidebar => self.reveal_in_sidebar(cx),
            Command::Shortcuts => self.open_picker(PaletteMode::Shortcuts, window, cx),
            Command::SaveFavorite => self.save_favorite(cx),
            Command::Favorites => self.open_picker(PaletteMode::Favorites, window, cx),
            Command::SaveChanges => self.request_save(cx),
            Command::DeleteRows if self.focus.is_focused(window) => self.request_delete(cx),
            Command::DeleteRows => {}
            Command::InsertRow if self.focus.is_focused(window) => self.open_insert_form(cx),
            Command::InsertRow => {}
            Command::Undo if self.focus.is_focused(window) => self.undo_edit(cx),
            Command::Redo if self.focus.is_focused(window) => self.redo_edit(cx),
            Command::Undo | Command::Redo => {}
            Command::Filter => {
                self.search.filter_visible = true;
                window.focus(&self.search.filter.read(cx).focus_handle(cx), cx);
            }
            Command::CopyCells if self.focus.is_focused(window) => self.copy_selection(false, cx),
            Command::CopyCellsWithHeaders if self.focus.is_focused(window) => self.copy_selection(true, cx),
            Command::CopyCells | Command::CopyCellsWithHeaders => {}
            Command::PasteCells
                if self.focus.is_focused(window)
                    && self.tabs.shows_grid_for(&self.catalog.grid_table)
                    && self.cell_edit.active.is_none() =>
            {
                self.paste_cells(cx)
            }
            Command::PasteCells => {}
            Command::ToggleSidebar => self.sidebar.open = !self.sidebar.open,
            Command::NewQuery => {
                self.tabs.open_query();
                self.sync_grid_owner();
                if self.live.is_some() {
                    self.focus_editor = true;
                } else {
                    window.focus(&self.focus, cx);
                }
            }
            Command::RunAtCursor if matches!(self.tabs.current(), Tab::Query) => self.run_at_cursor(cx),
            Command::RunAtCursor => {}
            Command::RunQuery if matches!(self.tabs.current(), Tab::Query) => self.run_query(cx),
            Command::RunQuery => {}
            Command::ToggleDetail => self.toggle_detail(),
            Command::CloseTab => {
                let index = self.tabs.active;
                self.close_tab(index, cx);
                self.show_current_table(window, cx);
            }
            Command::ReopenTab => self.reopen_closed_tab(window, cx),
            Command::RecentTab => self.switch_to_previous_tab(window, cx),
        }
    }

    pub(crate) fn grid_key(&mut self, key: &str, shift: bool, ctrl: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.visible_count();
        match key {
            "space" => {
                if let Some(cursor) = self.cell_cursor {
                    if self.toggle_boolean_at_cursor(window, cx) {
                        return;
                    }
                    self.click_row(self.base() + cursor.row as u64, false);
                }
            }
            "enter" | "f2" => {
                if let Some(cursor) = self.cell_cursor {
                    let source = self.source_row(self.base() + cursor.row as u64);
                    self.start_edit(source, cursor.column, window, cx);
                }
            }
            "down" if self.cell_cursor.is_some_and(|c| c.row + 1 == rows) && self.next_page() => {
                self.place_cursor(Some(Cursor {
                    row: 0,
                    column: self.cell_cursor.map_or(0, |c| c.column),
                }));
            }
            "up" if self.cell_cursor.is_some_and(|c| c.row == 0) && self.previous_page() => {
                let last = self.visible_count().saturating_sub(1);
                self.place_cursor(Some(Cursor {
                    row: last,
                    column: self.cell_cursor.map_or(0, |c| c.column),
                }));
            }
            _ => {
                let projection = self.table.read(cx).delegate().clone_projection();
                let current = self.cell_cursor.map(|cursor| Cursor {
                    row: cursor.row,
                    column: projection.visible_index(cursor.column).unwrap_or(0),
                });
                let Some(mut next) = move_cursor(current, key, rows, projection.len(), self.viewport_rows, ctrl) else {
                    return;
                };
                next.column = projection.source_index(next.column).unwrap_or(0);
                self.place_cursor(Some(next));
                if shift && matches!(key, "up" | "down") {
                    self.click_row(self.base() + next.row as u64, true);
                }
            }
        }
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Surface {
    Approval,
    HostKey,
    CloseConfirm,
    CancelProcess,
    DeleteConnection,
    Notifications,
    ConnectionForm,
    SaveReview,
    Import,
    ExportFile,
    SchemaDiagram,
    Insert,
    DataCompare,
    Structure,
    SetCells,
    Resolve,
    Backup,
    SqlFile,
    TablesExport,
    ConnectionBundle,
}

impl Surface {
    pub(crate) const ORDER: [Surface; 20] = [
        Surface::CancelProcess,
        Surface::CloseConfirm,
        Surface::HostKey,
        Surface::Approval,
        Surface::DeleteConnection,
        Surface::ConnectionForm,
        Surface::Notifications,
        Surface::Import,
        Surface::ExportFile,
        Surface::SchemaDiagram,
        Surface::SaveReview,
        Surface::Insert,
        Surface::DataCompare,
        Surface::Structure,
        Surface::SetCells,
        Surface::Resolve,
        Surface::Backup,
        Surface::SqlFile,
        Surface::TablesExport,
        Surface::ConnectionBundle,
    ];

    fn blocks_keys(self) -> bool {
        matches!(
            self,
            Surface::Approval
                | Surface::HostKey
                | Surface::CloseConfirm
                | Surface::CancelProcess
                | Surface::DeleteConnection
                | Surface::SaveReview
        )
    }
}

fn control_modifier(modifiers: gpui::Modifiers) -> bool {
    modifiers.control || (cfg!(target_os = "macos") && modifiers.platform)
}

fn palette_navigation_key(key: &str, modifiers: gpui::Modifiers) -> bool {
    matches!(key, "up" | "down" | "home" | "end" | "pageup" | "pagedown")
        && !(modifiers.shift || modifiers.control || modifiers.alt || modifiers.platform)
}

fn zoom_key(key: &str) -> Option<Option<i32>> {
    match key {
        "=" | "+" | "add" => Some(Some(1)),
        "-" | "subtract" => Some(Some(-1)),
        "0" => Some(None),
        _ => None,
    }
}

impl GridView {
    pub(crate) fn zoom_editor(&mut self, delta: Option<i32>, cx: &mut Context<Self>) {
        let current = self.fonts.settings.editor_size;
        let next = delta.map_or(font_settings::DEFAULT_ZOOM_SIZE, |delta| {
            font_settings::zoom_step(current, delta)
        });
        if next != current {
            self.edit_fonts(|fonts| fonts.editor_size = next, cx);
        }
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::zoom_key;

    #[test]
    fn plus_minus_and_zero_are_the_zoom_keys() {
        assert_eq!(zoom_key("="), Some(Some(1)));
        assert_eq!(zoom_key("+"), Some(Some(1)));
        assert_eq!(zoom_key("-"), Some(Some(-1)));
        assert_eq!(zoom_key("0"), Some(None));
        assert_eq!(zoom_key("a"), None);
    }
}
