use bookie_gpui::font_settings;
use bookie_gpui::keymap::{Chord, Command};
use bookie_results::grid_cursor::{Cursor, move_cursor};
use gpui::{App, Context, Focusable, KeyDownEvent, KeystrokeEvent, Window};

use crate::palette_state::PaletteMode;
use crate::{GridView, Tab, palette_step};

impl GridView {
    pub(super) fn intercept_key(&mut self, event: &KeystrokeEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.open {
            let key = event.keystroke.key.as_str();
            if matches!(key, "up" | "down" | "home" | "end" | "pageup" | "pagedown") {
                self.move_palette_selection(key, cx);
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        let modifiers = event.keystroke.modifiers;
        if !self.grid_key_ready(window, modifiers.control || modifiers.alt) {
            return;
        }
        let key = event.keystroke.key.as_str();
        if modifiers.shift && matches!(key, "up" | "down") {
            self.extend_selection_with_arrow(key, cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let plain = !(modifiers.shift || modifiers.control || modifiers.alt);
        if plain && self.cross_page_edge(key) {
            cx.stop_propagation();
            cx.notify();
        }
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
            self.palette.scroll.scroll_to_item(self.palette.index);
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

    pub(super) fn cross_page_edge(&mut self, key: &str) -> bool {
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

    pub(super) fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        let modifiers = stroke.modifiers;
        let key = stroke.key.as_str();
        if self.palette.open {
            match key {
                "down" | "up" | "home" | "end" | "pageup" | "pagedown" => {
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
                        self.palette.scroll.scroll_to_item(self.palette.index);
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
            && !modifiers.control
            && matches!(
                key,
                "up" | "down" | "left" | "right" | "home" | "end" | "pageup" | "pagedown"
            )
        {
            return;
        }
        if modifiers.control && matches!(self.tabs.current(), Tab::Query) {
            if let Some(delta) = zoom_key(key) {
                self.zoom_editor(delta, cx);
                return;
            }
        }
        let chord = Chord::new(modifiers.control, modifiers.shift, modifiers.alt, key);
        if let Some(command) = self.keymap.resolve(&chord) {
            self.run_command(command, window, cx);
            cx.notify();
            return;
        }
        if modifiers.control && !matches!(key, "home" | "end") {
            cx.notify();
            return;
        }
        if self.focus.is_focused(window)
            && self.cell_edit.active.is_none()
            && self.compare.is_none()
            && self.tabs.shows_grid_for(&self.catalog.grid_table)
        {
            self.grid_key(key, modifiers.shift, modifiers.control, window, cx);
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
            Command::PasteCells if self.focus.is_focused(window) && self.tabs.shows_grid_for(&self.catalog.grid_table) && self.cell_edit.active.is_none() => self.paste_cells(cx),
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
                self.close_tab(index);
                self.show_current_table(window, cx);
            }
            Command::ReopenTab => self.reopen_closed_tab(window, cx),
            Command::RecentTab => self.switch_to_previous_tab(window, cx),
        }
    }

    pub(super) fn grid_key(&mut self, key: &str, shift: bool, ctrl: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.visible_count();
        match key {
            "space" => {
                if let Some(cursor) = self.cell_cursor {
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
