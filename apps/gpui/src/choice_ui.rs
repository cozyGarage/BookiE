use bookie_core::ColumnInfo;
use bookie_gpui::model::Cell;
use gpui::{Context, DismissEvent, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

use crate::GridView;
use crate::choice_edit::{Choice, choice_for, is_choice_candidate, member_cell};
use crate::grid_edit::EditMode;
use crate::menus::RowMenu;

impl GridView {
    pub(crate) fn begin_choice_edit(
        &mut self,
        row: u64,
        column: usize,
        cell: &Cell,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(meta) = self.source.columns().get(column) else {
            return false;
        };
        if !is_choice_candidate(&meta.data_type, cell) {
            return false;
        }
        let Some(live) = self.live.clone() else {
            return false;
        };
        let table = self.catalog.grid_table.clone();
        if let Some(columns) = live.cached_columns(&table) {
            self.apply_choice(row, column, &columns, window, cx);
            return true;
        }
        let asked = (self.save.generation, table.clone());
        cx.spawn_in(window, async move |this, cx| {
            let columns = cx
                .background_executor()
                .spawn(async move { live.table_columns(&table) })
                .await;
            this.update_in(cx, |this, window, cx| {
                let now = (this.save.generation, this.catalog.grid_table.clone());
                if now != asked || this.cell_edit.active.is_some() || this.form_open() {
                    return;
                }
                this.apply_choice(row, column, &columns.unwrap_or_default(), window, cx);
            })
            .ok();
        })
        .detach();
        true
    }

    pub(crate) fn toggle_boolean_at_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(cursor) = self.cell_cursor else {
            return false;
        };
        if self.live.is_none() || self.compare.is_some() || cursor.row >= self.visible_count() {
            return false;
        }
        let row = self.source_row(self.base() + cursor.row as u64);
        if !matches!(self.cell_ref_at(row, cursor.column), Some(Cell::Bool(_))) {
            return false;
        }
        self.start_edit(row, cursor.column, window, cx);
        true
    }

    fn apply_choice(
        &mut self,
        row: u64,
        column: usize,
        columns: &[ColumnInfo],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(meta) = self.source.columns().get(column) else {
            return;
        };
        let info = columns.iter().find(|info| info.name == meta.name);
        let cell = self.cell_at(row, column);
        match choice_for(&meta.data_type, info, &cell) {
            Some(Choice::Toggle(next)) => self.stage_choice(row, column, next, cx),
            Some(Choice::Members(labels)) => match info {
                Some(info) => self.open_choice_menu((row, column), info.clone(), labels, window, cx),
                None => self.start_edit_mode(row, column, EditMode::Inline, window, cx),
            },
            None => self.start_edit_mode(row, column, EditMode::Inline, window, cx),
        }
    }

    pub(crate) fn stage_choice(&mut self, row: u64, column: usize, cell: Cell, cx: &mut Context<Self>) {
        if self.save.pending.is_some() || self.save.busy {
            self.status = "Save or cancel the review before editing more cells".into();
            cx.notify();
            return;
        }
        if self.cell_at(row, column) == cell {
            return;
        }
        self.record_edit((row, column), cell);
        self.edits_changed();
        self.status = self.pending_edit_status();
        cx.notify();
    }

    fn pick_member(&mut self, row: u64, column: usize, label: &str, info: &ColumnInfo, cx: &mut Context<Self>) {
        match member_cell(label, info) {
            Ok(cell) => self.stage_choice(row, column, cell, cx),
            Err(reason) => {
                self.status = format!("Not applied: {reason}");
                cx.notify();
            }
        }
    }

    fn open_choice_menu(
        &mut self,
        (row, column): (u64, usize),
        info: ColumnInfo,
        labels: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        let current = match self.cell_at(row, column) {
            Cell::Text(text) => Some(text),
            _ => None,
        };
        let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
            menu = menu.scrollable(true);
            for label in &labels {
                let (owner, info, picked) = (owner.clone(), info.clone(), label.clone());
                let item = PopupMenuItem::new(label.clone())
                    .checked(current.as_deref() == Some(label.as_str()))
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| this.pick_member(row, column, &picked, &info, cx));
                    });
                menu = menu.item(item);
            }
            menu
        });
        let dismiss = cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.row_menu = None;
            cx.notify();
        });
        window.focus(&gpui::Focusable::focus_handle(menu.read(cx), cx), cx);
        self.row_menu = Some(RowMenu::new(window.mouse_position(), menu, dismiss));
        cx.notify();
    }
}
