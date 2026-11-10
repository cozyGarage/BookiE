use bookie_gpui::cursor::{Cursor, move_cursor};
use bookie_gpui::keymap::{Chord, Command};
use gpui::{Context, Focusable, KeyDownEvent, KeystrokeEvent, Window};

use crate::{GridView, Tab, palette_step};

impl GridView {
    /// Runs before key bindings, so the table's own arrow actions never see an edge key we handle.
    pub(super) fn intercept_key(&mut self, event: &KeystrokeEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        let plain = !(modifiers.shift || modifiers.control || modifiers.alt);
        let ready = self.focus.is_focused(window)
            && plain
            && self.editing.is_none()
            && self.compare.is_none()
            && !self.continuous
            && !self.palette_open
            && self.tabs.shows_grid();
        if ready && self.cross_page_edge(event.keystroke.key.as_str()) {
            cx.stop_propagation();
            cx.notify();
        }
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
        if self.palette_open {
            match key {
                "down" | "up" => {
                    let count = self.palette_shown(cx).len();
                    self.palette_index = palette_step(self.palette_index, count, key == "down");
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
        if self.focus.is_focused(window) && self.editing.is_none() && self.compare.is_none() && self.tabs.shows_grid() {
            self.grid_key(key, modifiers.shift, modifiers.control, window, cx);
        }
    }

    fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::Palette => self.open_palette(window, cx),
            Command::Filter => {
                self.filter_visible = true;
                window.focus(&self.filter.read(cx).focus_handle(cx), cx);
            }
            Command::CopyCells if self.focus.is_focused(window) => self.copy_selection(false, cx),
            Command::CopyCellsWithHeaders if self.focus.is_focused(window) => self.copy_selection(true, cx),
            Command::CopyCells | Command::CopyCellsWithHeaders => {}
            Command::ToggleSidebar => self.sidebar_open = !self.sidebar_open,
            Command::NewQuery => {
                self.tabs.open_query();
                self.sync_grid_owner();
                window.focus(&self.focus, cx);
            }
            Command::RunQuery if matches!(self.tabs.current(), Tab::Query) => self.run_query(cx),
            Command::RunQuery => {}
            Command::ToggleDetail => self.toggle_detail(),
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
                let Some(next) = move_cursor(
                    self.cell_cursor,
                    key,
                    rows,
                    self.source.columns().len(),
                    self.viewport_rows,
                    ctrl,
                ) else {
                    return;
                };
                self.place_cursor(Some(next));
                if shift && matches!(key, "up" | "down") {
                    self.click_row(self.base() + next.row as u64, true);
                }
            }
        }
        cx.notify();
    }
}
