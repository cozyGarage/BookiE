use std::sync::{Arc, Weak};

use bookie_core::{ColumnInfo, QualifiedTypeName};
use bookie_gpui::model::Cell;
use bookie_gpui::shell::Tab;
use bookie_gpui::source::ResultSource;
use gpui::{Context, Window};
use gpui_component::menu::PopupMenu;

use crate::GridView;
use crate::choice_edit::{Choice, choice_for, is_choice_candidate, member_cell};
use crate::grid_edit::EditMode;

#[derive(Clone)]
struct ChoiceTarget {
    source: Weak<dyn ResultSource>,
    tab: Tab,
    generation: u64,
    request: u64,
    row: u64,
    column: usize,
}

impl ChoiceTarget {
    fn is_current(&self, view: &GridView) -> bool {
        self.tab == *view.tabs.current()
            && self.generation == view.save.generation
            && self.request == view.cell_edit.choice_request
            && self
                .source
                .upgrade()
                .is_some_and(|source| Arc::ptr_eq(&source, &view.source))
            && view.source.editable()
            && view.compare.is_none()
            && !view.form_open()
            && view.cell_edit.active.is_none()
            && self.row < view.source.total()
            && self.column < view.source.columns().len()
    }
}

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
        let Some(live) = self.live.clone() else {
            return false;
        };
        let possible_postgres_enum = live.driver_id == "postgres" && matches!(cell, Cell::Text(_) | Cell::Null);
        if !possible_postgres_enum && !is_choice_candidate(&meta.data_type, cell) {
            return false;
        }
        let table = self.catalog.grid_table.clone();
        let target = ChoiceTarget {
            source: Arc::downgrade(&self.source),
            tab: self.tabs.current().clone(),
            generation: self.save.generation,
            request: self.cell_edit.choice_request,
            row,
            column,
        };
        if let Some(columns) = live.cached_columns(&table) {
            self.apply_choice(target, &columns, window, cx);
            return true;
        }
        cx.spawn_in(window, async move |this, cx| {
            let columns = cx
                .background_executor()
                .spawn(async move { live.table_columns(&table) })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.apply_choice(target, &columns.unwrap_or_default(), window, cx);
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
        target: ChoiceTarget,
        columns: &[ColumnInfo],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !target.is_current(self) {
            return;
        }
        let (row, column) = (target.row, target.column);
        let meta = &self.source.columns()[column];
        let info = columns.iter().find(|info| info.name == meta.name);
        let cell = self.cell_at(row, column);
        match choice_for(&meta.data_type, info, &cell) {
            Some(Choice::Toggle(next)) => self.stage_choice(row, column, next, cx),
            Some(Choice::Members(labels)) => self.open_choice_menu(target, labels, window, cx),
            Some(Choice::PostgresEnum(type_name)) => self.read_choice_labels(target, type_name, window, cx),
            None => self.start_edit_mode(row, column, EditMode::Inline, window, cx),
        }
    }

    fn read_choice_labels(
        &mut self,
        target: ChoiceTarget,
        type_name: QualifiedTypeName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.clone() else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let labels = cx
                .background_executor()
                .spawn(async move { live.enum_labels(&type_name) })
                .await;
            this.update_in(cx, |this, window, cx| {
                if !target.is_current(this) {
                    return;
                }
                match labels {
                    Ok(labels) if !labels.is_empty() => this.open_choice_menu(target, labels, window, cx),
                    Ok(_) => this.start_edit_mode(target.row, target.column, EditMode::Inline, window, cx),
                    Err(reason) => {
                        this.status = format!("Could not load enum values: {reason}");
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn stage_choice(&mut self, row: u64, column: usize, cell: Cell, cx: &mut Context<Self>) {
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

    fn pick_member(&mut self, target: &ChoiceTarget, label: &str, labels: &[String], cx: &mut Context<Self>) {
        if target.is_current(self)
            && let Some(cell) = member_cell(label, labels)
        {
            self.stage_choice(target.row, target.column, cell, cx);
        }
    }

    fn open_choice_menu(
        &mut self,
        target: ChoiceTarget,
        labels: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        let current = match self.cell_at(target.row, target.column) {
            Cell::Text(text) => Some(text),
            _ => None,
        };
        let labels: Arc<[String]> = labels.into();
        let options: Vec<(gpui::SharedString, String)> = labels
            .iter()
            .map(|label| (label.clone().into(), label.clone()))
            .collect();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            let (owner, target, labels) = (owner.clone(), target.clone(), labels.clone());
            crate::menus::select(
                menu.scrollable(true),
                &options,
                current.as_ref(),
                move |picked: String, _, cx| {
                    let _ = owner.update(cx, |this, cx| this.pick_member(&target, &picked, &labels, cx));
                },
            )
        });
        self.show_row_menu(menu, window, cx);
    }
}
