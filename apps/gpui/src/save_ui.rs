use std::sync::Arc;

use bookie_app_services::cell_save_plan::{CellChange, CellSavePlan, SaveTarget, plan_cell_saves};
use bookie_gpui::live::{SaveFailure, WriteSession};

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
    pub changes: usize,
    pub generation: u64,
    pub table: bookie_core::TableInfo,
}

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

    pub(crate) fn request_save(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone().filter(|_| self.can_save_edits()) else {
            self.status = "Saving needs a table from a saved connection that is not read-only".into();
            return;
        };
        if self.cell_edit.edits.is_empty() {
            self.status = "No changes to save".into();
            return;
        }
        if self.save.busy || self.save.pending.is_some() {
            return;
        }
        if self.cell_edit.active.is_some() {
            self.status = "Finish editing the cell first, then press Ctrl+S".into();
            return;
        }
        self.save.busy = true;
        let generation = self.save.generation;
        let table = self.catalog.grid_table.clone();
        let grid_columns = self.column_names();
        let changes = self.pending_changes();
        let count = changes.len();
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
                    plan_cell_saves(target, &grid_columns, &changes).map_err(|error| error.to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                this.receive_plan(planned, (count, generation, planned_table));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn receive_plan(
        &mut self,
        planned: Result<CellSavePlan, String>,
        (count, generation, table): (usize, u64, bookie_core::TableInfo),
    ) {
        self.save.busy = false;
        if self.save.generation != generation {
            self.status = "The edits changed while the changes were prepared; press Ctrl+S again".into();
            return;
        }
        match planned {
            Ok(plan) if plan.statements.is_empty() => {
                self.cell_edit.edits.clear();
                self.status = "Nothing changed, so there is nothing to save".into();
            }
            Ok(plan) => {
                self.status = format!("Review {count} change(s), then save or cancel");
                self.save.pending = Some(PendingSave {
                    plan,
                    changes: count,
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
        let count = pending.changes;
        let (statements, expect_one) = (pending.plan.statements, pending.plan.expect_one);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let write = match write {
                        Some(write) => write,
                        None => Arc::new(WriteSession::open_saved(connection_id)?),
                    };
                    let applied = write.apply(&statements, &expect_one, &tokio_util::sync::CancellationToken::new());
                    Ok::<_, String>((write, applied))
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_save(connection_id, count, result, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn finish_save(&mut self, connection_id: uuid::Uuid, count: usize, result: SaveOutcome, cx: &mut Context<Self>) {
        let (write, applied) = match result {
            Ok(done) => done,
            Err(message) => {
                self.save.busy = false;
                self.status = format!("Could not open a connection for saving: {message}");
                self.show_banner(self.status.clone(), true, cx);
                return;
            }
        };
        self.save.write = Some((connection_id, write));
        match applied {
            Ok(_) => {
                self.show_banner(format!("Saved {count} change(s)"), false, cx);
                self.reload_after_save(format!("Saved {count} change(s)"), cx);
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
        let driver = self.live.as_ref().map_or("", |live| live.driver_id.as_str());
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
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(format!("Save {} change(s) to {driver}?", pending.changes)),
                        )
                        .child(
                            primary_button("confirm-save", "Save", "Run these statements in one transaction", theme)
                                .on_click(cx.listener(|this, _, _, cx| this.confirm_save(cx))),
                        )
                        .child(
                            ghost("cancel-save", "Cancel", theme)
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_save(cx))),
                        ),
                )
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
