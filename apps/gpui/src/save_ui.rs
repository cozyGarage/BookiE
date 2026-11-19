use std::sync::Arc;

use bookie_app_services::cell_save_plan::{
    CellChange, CellSavePlan, SaveTarget, plan_cell_saves, plan_row_deletes, plan_row_insert_named,
};
use bookie_gpui::live::{LiveSession, SaveFailure, WriteSession};

use super::*;
use crate::chrome::{ghost, primary_button};

type SaveOutcome = Result<(Arc<WriteSession>, Result<Vec<u64>, SaveFailure>), String>;

#[derive(Default)]
pub(crate) struct SaveState {
    pub write: Option<(uuid::Uuid, Arc<WriteSession>)>,
    pub pending: Option<PendingSave>,
    pub busy: bool,
    pub generation: u64,
    pub banner: Option<(String, bool)>,
    pub banner_serial: u64,
}

pub(crate) struct PendingSave {
    pub plan: CellSavePlan,
    pub title: String,
    pub done: String,
    pub confirm: &'static str,
    pub note: Option<&'static str>,
    pub generation: u64,
    pub table: bookie_core::TableInfo,
}

pub(crate) enum SaveRequest {
    Edits(Vec<CellChange>),
    Deletes(Vec<Vec<bookie_core::Value>>),
    Insert(Vec<(String, String)>),
}

pub(crate) struct ReviewLabels {
    pub title: String,
    pub done: String,
    pub review: String,
    pub confirm: &'static str,
    pub note: Option<&'static str>,
}

const MAX_DELETE_ROWS: u64 = 500;

impl GridView {
    pub(crate) fn edits_changed(&mut self) {
        self.save.generation += 1;
        self.save.pending = None;
    }

    pub(crate) fn can_save_edits(&self) -> bool {
        self.live.as_ref().is_some_and(|live| live.writable.is_some()) && self.source.editable()
    }

    fn pending_changes(&self) -> Vec<CellChange> {
        self.cell_edit
            .edits
            .iter()
            .map(|(&(row, column), cell)| CellChange {
                original_row: self.source.row(row),
                column,
                new_value: cell.clone(),
            })
            .collect()
    }

    pub(crate) fn save_session(&mut self) -> Option<Arc<LiveSession>> {
        let Some(live) = self.live.clone().filter(|_| self.can_save_edits()) else {
            self.status = "Saving needs a table from a saved connection that is not read-only".into();
            return None;
        };
        if self.save.busy || self.save.pending.is_some() {
            self.status = "A save is already running or waiting for your review".into();
            return None;
        }
        if self.cell_edit.active.is_some() {
            self.status = "Finish editing the cell first".into();
            return None;
        }
        Some(live)
    }

    pub(crate) fn request_save(&mut self, cx: &mut Context<Self>) {
        if self.insert.open {
            self.status = "Finish or cancel the new row first".into();
            return;
        }
        if self.cell_edit.edits.is_empty() && self.live.as_ref().is_some_and(|live| live.writable.is_some()) {
            self.status = "No changes to save".into();
            return;
        }
        let Some(live) = self.save_session() else {
            return;
        };
        let changes = self.pending_changes();
        let count = changes.len();
        let labels = ReviewLabels {
            title: format!("Save {count} change(s) to {}?", live.driver_id),
            done: format!("Saved {count} change(s)"),
            review: format!("Review {count} change(s), then save or cancel"),
            confirm: "Save",
            note: None,
        };
        self.prepare(live, SaveRequest::Edits(changes), labels, cx);
    }

    pub(crate) fn request_delete(&mut self, cx: &mut Context<Self>) {
        if self.insert.open {
            self.status = "Finish or cancel the new row first".into();
            return;
        }
        if self.compare.is_some() {
            self.status = "Leave compare mode before deleting rows".into();
            return;
        }
        let Some(live) = self.save_session() else {
            return;
        };
        let cursor_row = self.cell_cursor.map(|cursor| {
            let row = self.base() + cursor.row as u64;
            bookie_gpui::selection::Selection {
                anchor: row,
                focus: row,
            }
        });
        let Some(selection) = self.selection.or(cursor_row) else {
            self.status = "Select the rows to delete first".into();
            return;
        };
        if !self.cell_edit.edits.is_empty() {
            self.status = "Save or discard the pending edits before deleting rows".into();
            return;
        }
        if selection.count() > MAX_DELETE_ROWS {
            self.status = format!("Select at most {MAX_DELETE_ROWS} rows to delete at once");
            return;
        }
        let rows = self.selected_rows(selection);
        let count = rows.len();
        let labels = ReviewLabels {
            title: format!("Delete {count} row(s) from {}?", self.catalog.grid_table.name),
            done: format!("Deleted {count} row(s)"),
            review: format!("Review the {count} row(s) to delete, then delete or cancel"),
            confirm: "Delete",
            note: Some(
                "Rows that depend on these (foreign keys, triggers) may be affected too. \
                 A connection that needs approval will refuse this until approval is available here.",
            ),
        };
        self.prepare(live, SaveRequest::Deletes(rows), labels, cx);
    }

    pub(crate) fn prepare(
        &mut self,
        live: Arc<LiveSession>,
        request: SaveRequest,
        labels: ReviewLabels,
        cx: &mut Context<Self>,
    ) {
        self.save.busy = true;
        let generation = self.save.generation;
        let table = self.catalog.grid_table.clone();
        let grid_columns = self.column_names();
        self.status = "Preparing the changes…".into();
        cx.spawn(async move |this, cx| {
            let planned_table = table.clone();
            let planned = cx
                .background_executor()
                .spawn(async move {
                    let columns = live.table_columns(&table)?;
                    let target = SaveTarget {
                        driver_id: &live.driver_id,
                        schema: table.schema.as_deref(),
                        table: &table.name,
                        columns: &columns,
                    };
                    match request {
                        SaveRequest::Edits(changes) => plan_cell_saves(target, &grid_columns, &changes),
                        SaveRequest::Deletes(rows) => plan_row_deletes(target, &grid_columns, &rows),
                        SaveRequest::Insert(named) => plan_row_insert_named(target, &named),
                    }
                    .map_err(|error| error.to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                this.receive_plan(planned, labels, (generation, planned_table));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn receive_plan(
        &mut self,
        planned: Result<CellSavePlan, String>,
        labels: ReviewLabels,
        (generation, table): (u64, bookie_core::TableInfo),
    ) {
        self.save.busy = false;
        if self.save.generation != generation {
            self.status = "The edits changed while the changes were prepared; try again".into();
            return;
        }
        match planned {
            Ok(plan) if plan.statements.is_empty() => {
                self.cell_edit.edits.clear();
                self.reset_edit_history();
                self.status = "Nothing changed, so there is nothing to save".into();
            }
            Ok(plan) => {
                self.status = labels.review;
                self.save.pending = Some(PendingSave {
                    plan,
                    title: labels.title,
                    done: labels.done,
                    confirm: labels.confirm,
                    note: labels.note,
                    generation,
                    table,
                });
            }
            Err(message) => self.status = format!("Cannot save: {message}"),
        }
    }

    fn show_banner(&mut self, text: String, is_error: bool, cx: &mut Context<Self>) {
        self.save.banner_serial += 1;
        let serial = self.save.banner_serial;
        self.save.banner = Some((text, is_error));
        if is_error {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_secs(5)).await;
            this.update(cx, |this, cx| {
                if this.save.banner_serial == serial {
                    this.save.banner = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn dismiss_save_banner(&mut self, cx: &mut Context<Self>) {
        self.save.banner = None;
        cx.notify();
    }

    pub(crate) fn cancel_save(&mut self, cx: &mut Context<Self>) {
        if self.save.pending.take().is_some() {
            self.status = "Save cancelled; the edits are still pending".into();
            cx.notify();
        }
    }

    pub(crate) fn confirm_save(&mut self, cx: &mut Context<Self>) {
        let (Some(pending), Some(live)) = (self.save.pending.take(), self.live.clone()) else {
            return;
        };
        let Some(connection_id) = live.writable else {
            return;
        };
        let on_the_table = matches!(self.tabs.current(), Tab::Table(table) if *table == self.catalog.grid_table);
        if self.save.busy
            || pending.generation != self.save.generation
            || pending.table != self.catalog.grid_table
            || !on_the_table
        {
            self.status = "The table or the edits changed since the review; press Ctrl+S to review again".into();
            cx.notify();
            return;
        }
        self.save.busy = true;
        self.status = "Saving…".into();
        let write = self
            .save
            .write
            .as_ref()
            .filter(|(id, _)| *id == connection_id)
            .map(|(_, write)| write.clone());
        let approvals = self.approval.tx.clone();
        let done = pending.done;
        let (statements, expect_one) = (pending.plan.statements, pending.plan.expect_one);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let write = match write.filter(|write| write.is_current().unwrap_or(false)) {
                        Some(write) => write,
                        None => Arc::new(WriteSession::open_saved(connection_id, approvals)?),
                    };
                    let applied = write.apply(&statements, &expect_one, &tokio_util::sync::CancellationToken::new());
                    Ok::<_, String>((write, applied))
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_save(connection_id, done, result, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn finish_save(&mut self, connection_id: uuid::Uuid, done: String, result: SaveOutcome, cx: &mut Context<Self>) {
        let (write, applied) = match result {
            Ok(done) => done,
            Err(message) => {
                self.save.busy = false;
                self.status = format!("Could not open a connection for saving: {message}");
                self.show_banner(self.status.clone(), true, cx);
                return;
            }
        };
        self.save.write = applied.is_ok().then(|| (connection_id, write));
        match applied {
            Ok(_) => {
                self.close_insert_form();
                self.show_banner(done.clone(), false, cx);
                self.reload_after_save(done, cx);
            }
            Err(failure) if failure.outcome_unknown => {
                let text = format!(
                    "The save may or may not have been applied ({}). The table was reloaded; check the data before saving again.",
                    failure.message
                );
                self.show_banner(text.clone(), true, cx);
                self.reload_after_save(text, cx);
            }
            Err(failure) => {
                self.save.busy = false;
                self.status = format!("Could not save: {}", failure.message);
                self.show_banner(self.status.clone(), true, cx);
            }
        }
    }

    pub(crate) fn save_banner(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let (text, is_error) = self.save.banner.clone()?;
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .when(is_error, |row| row.text_color(rgb(theme.destructive)))
                .child(div().flex_1().child(text))
                .child(
                    ghost("dismiss-save-banner", "Dismiss", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.dismiss_save_banner(cx);
                    })),
                ),
        )
    }

    pub(crate) fn save_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let pending = self.save.pending.as_ref()?;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().font_weight(gpui::FontWeight::MEDIUM).child(pending.title.clone()))
                        .child(
                            primary_button(
                                "confirm-save",
                                pending.confirm,
                                "Run these statements in one transaction",
                                theme,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.confirm_save(cx))),
                        )
                        .child(
                            ghost("cancel-save", "Cancel", theme)
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_save(cx))),
                        ),
                )
                .children(pending.note.map(|note| {
                    div()
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child(note)
                }))
                .child(
                    div()
                        .id("save-preview")
                        .max_h(px(size::SAVE_PREVIEW_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .font_family("monospace")
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child(pending.plan.preview.clone()),
                ),
        )
    }
}
