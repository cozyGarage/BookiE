use std::sync::Arc;

use bookie_app_services::cell_save_plan::{
    CellChange, CellSavePlan, SaveTarget, plan_cell_saves, plan_row_deletes, plan_row_insert_named,
};
use bookie_app_services::csv_import_plan::plan_csv_import;
use bookie_gpui::live::{LiveSession, SaveFailure, WriteSession};
use bookie_gpui::messages::plural;
use bookie_gpui::save_check::{CellCheck, cell_checks, key_columns, verify};
use bookie_gpui::source::ViewAccess;

use crate::surfaces::tip;
use crate::surfaces::{ButtonKind, button, compact_button, sql_preview, task_bar_frame, task_title};
use crate::*;

type PreparedSave = (CellSavePlan, Option<ForeignKeyReview>);

struct ForeignKeyReview {
    source: bookie_core::TableInfo,
    target: bookie_core::TableInfo,
    source_columns: Vec<bookie_core::ColumnInfo>,
    target_columns: Vec<bookie_core::ColumnInfo>,
}

impl ForeignKeyReview {
    fn verify(&self, live: &LiveSession) -> Result<(), String> {
        if live.diagram_columns(&self.source)? != self.source_columns
            || live.diagram_columns(&self.target)? != self.target_columns
        {
            return Err("The relationship tables changed since review. Refresh the diagram and review again".into());
        }
        Ok(())
    }
}

type SaveOutcome = Result<(Arc<WriteSession>, Result<Vec<u64>, SaveFailure>), String>;

#[derive(Default)]
pub(crate) struct SaveState {
    pub write: Option<(uuid::Uuid, Arc<WriteSession>)>,
    pub pending: Option<PendingSave>,
    pub busy: bool,
    pub generation: u64,
    pub banner: Option<(String, bool)>,
    pub banner_serial: u64,
    refresh_diagram: Option<Option<String>>,
    pub reconnect_after: Option<bookie_core::TableInfo>,
    pub diagram_after_reconnect: Option<Option<String>>,
    pub checks: Vec<CellCheck>,
}

pub(crate) struct PendingSave {
    relationship: Option<ForeignKeyReview>,
    diagram: Option<(u64, Option<Vec<bookie_core::ColumnInfo>>)>,
    pub plan: CellSavePlan,
    pub title: String,
    pub done: String,
    pub confirm: &'static str,
    pub destructive: bool,
    pub note: Option<&'static str>,
    pub reconnect: Option<bookie_core::TableInfo>,
    pub generation: u64,
    pub table: bookie_core::TableInfo,
    pub checks: Vec<CellCheck>,
}

pub(crate) enum SaveRequest {
    Edits(Vec<CellChange>),
    Deletes(Vec<Vec<bookie_core::Value>>),
    Insert(Vec<(String, String)>),
    Import(std::path::PathBuf, bookie_app_services::csv_import_plan::ImportChoices),
    Structure(bookie_app_services::structure_change_plan::StructureChangeKind, String),
}

pub(crate) struct ReviewLabels {
    pub title: String,
    pub done: String,
    pub review: String,
    pub confirm: &'static str,
    pub destructive: bool,
    pub note: Option<&'static str>,
    pub reconnect: Option<bookie_core::TableInfo>,
}

const MAX_DELETE_ROWS: u64 = 500;

fn view_refusal(request: &SaveRequest, access: &ViewAccess) -> Option<String> {
    match request {
        SaveRequest::Edits(_) | SaveRequest::Deletes(_) => access.edit_refusal(),
        SaveRequest::Insert(_) | SaveRequest::Import(..) => access.insert_refusal(),
        SaveRequest::Structure(..) => None,
    }
}

pub(crate) const TRANSACTION_BLOCKS_SAVE: &str = "Commit or roll back the open transaction first";

impl GridView {
    pub(crate) fn edits_changed(&mut self) {
        self.save.generation += 1;
        self.save.pending = None;
    }

    pub(crate) fn can_save_edits(&self) -> bool {
        self.is_writable() && self.source.editable()
    }

    pub(crate) fn view_edit_refusal(&self) -> Option<String> {
        self.source.view_access().and_then(ViewAccess::edit_refusal)
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
        if self.transaction_open() {
            self.status = TRANSACTION_BLOCKS_SAVE.into();
            return None;
        }
        let Some(live) = self.live.clone().filter(|_| self.can_save_edits()) else {
            self.status = self
                .view_edit_refusal()
                .unwrap_or_else(|| "Saving needs a table from a saved connection that is not read-only".into());
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
        if self.form_open() {
            self.status = "Finish or cancel the open form first".into();
            return;
        }
        if self.cell_edit.edits.is_empty() && self.is_writable() {
            self.status = "No changes to save".into();
            return;
        }
        let Some(live) = self.save_session() else {
            return;
        };
        let changes = self.pending_changes();
        let count = changes.len();
        let labels = ReviewLabels {
            title: format!("Save {} to {}?", plural(count, "change", "changes"), live.driver_id),
            done: format!("Saved {}", plural(count, "change", "changes")),
            review: format!("Review {}, then save or cancel", plural(count, "change", "changes")),
            confirm: "Save",
            destructive: false,
            note: None,
            reconnect: None,
        };
        self.prepare(live, SaveRequest::Edits(changes), labels, cx);
    }

    pub(crate) fn request_delete(&mut self, cx: &mut Context<Self>) {
        if self.form_open() {
            self.status = "Finish or cancel the open form first".into();
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
            title: format!(
                "Delete {} from {}?",
                plural(count, "row", "rows"),
                self.catalog.grid_table.name
            ),
            done: format!("Deleted {}", plural(count, "row", "rows")),
            review: format!(
                "Review the {} to delete, then delete or cancel",
                plural(count, "row", "rows")
            ),
            confirm: "Delete",
            destructive: true,
            note: Some("Rows that depend on these (foreign keys, triggers) may be affected too"),
            reconnect: None,
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
        if self.transaction_open() {
            self.status = TRANSACTION_BLOCKS_SAVE.into();
            return;
        }
        self.save.busy = true;
        let generation = self.save.generation;
        let table = self.catalog.grid_table.clone();
        let grid_columns = self.column_names();
        let checks = match &request {
            SaveRequest::Edits(changes) => cell_checks(
                &key_columns(self.source.columns()),
                changes
                    .iter()
                    .map(|change| (&change.original_row[..], change.column, &change.new_value)),
            ),
            _ => Vec::new(),
        };
        self.status = "Preparing the changes…".into();
        cx.spawn(async move |this, cx| {
            let planned_table = table.clone();
            let planned = cx
                .background_executor()
                .spawn(async move {
                    let (columns, access) = live.edit_columns(&table)?;
                    if let Some(refusal) = view_refusal(&request, &access) {
                        return Err(refusal);
                    }
                    let target = SaveTarget {
                        driver_id: &live.driver_id,
                        schema: table.schema.as_deref(),
                        table: &table.name,
                        columns: &columns,
                    };
                    let planned = match request {
                        SaveRequest::Edits(changes) => {
                            plan_cell_saves(target, &grid_columns, &changes).map_err(|error| error.to_string())
                        }
                        SaveRequest::Deletes(rows) => {
                            plan_row_deletes(target, &grid_columns, &rows).map_err(|error| error.to_string())
                        }
                        SaveRequest::Insert(named) => {
                            let driver = drivers_registry::default_registry()
                                .get(&live.driver_id)
                                .ok_or_else(|| "The database driver is unavailable".to_string())?;
                            let hooks = bookie_app_services::value_input::ParseHooks::new(
                                driver.capabilities().enum_input_style,
                                driver.value_input_capabilities(),
                            );
                            plan_row_insert_named(target, &named, &hooks).map_err(|error| error.to_string())
                        }
                        SaveRequest::Import(file, choices) => {
                            plan_file_import(target, &live.driver_id, &file, &choices)
                        }
                        SaveRequest::Structure(kind, text) => {
                            let structure = if kind.needs_structure() {
                                live.table_structure(&table)?
                            } else {
                                Default::default()
                            };
                            plan_structure(target, kind, &text, &structure)
                        }
                    };
                    planned.map(|plan| (plan, None))
                })
                .await;
            this.update(cx, |this, cx| {
                this.receive_plan(planned, labels, (generation, planned_table), checks);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn prepare_diagram_ddl(
        &mut self,
        live: Arc<LiveSession>,
        table: bookie_core::TableInfo,
        create: bool,
        plan: CellSavePlan,
        labels: ReviewLabels,
        cx: &mut Context<Self>,
    ) {
        self.prepare_diagram(
            live,
            table,
            labels,
            move |live, target| {
                let columns = if create {
                    None
                } else {
                    Some(live.diagram_columns(target)?)
                };
                Ok(((plan, None), columns))
            },
            cx,
        );
    }

    pub(crate) fn review_diagram_foreign_key(
        &mut self,
        table: bookie_core::TableInfo,
        key: bookie_core::ForeignKeyInfo,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.clone().filter(|live| live.writable.is_some()) else {
            self.status = "Adding a relationship needs a writable saved connection".into();
            cx.notify();
            return;
        };
        if !matches!(self.tabs.current(), Tab::Diagram)
            || self.transaction_open()
            || self.save.busy
            || self.save.pending.is_some()
            || self.form_open()
        {
            self.status = "Finish the open transaction or form before reviewing a relationship".into();
            cx.notify();
            return;
        }
        let name = table_label(&table);
        let labels = ReviewLabels {
            title: format!("Add foreign key {} on {name}?", key.name),
            done: format!("Added foreign key {} on {name}", key.name),
            review: "Review the relationship change, then apply or cancel".into(),
            confirm: "Add foreign key",
            destructive: false,
            note: Some("The database checks existing rows and may block writes while validating the new constraint."),
            reconnect: None,
        };
        self.prepare_diagram(
            live,
            table,
            labels,
            move |live, target| {
                let source_columns = live.diagram_columns(target)?;
                prepare_diagram_foreign_key(live, target, source_columns, key).map(|plan| (plan, None))
            },
            cx,
        );
    }

    fn prepare_diagram(
        &mut self,
        live: Arc<LiveSession>,
        table: bookie_core::TableInfo,
        labels: ReviewLabels,
        plan: impl FnOnce(
            &LiveSession,
            &bookie_core::TableInfo,
        ) -> Result<(PreparedSave, Option<Vec<bookie_core::ColumnInfo>>), String>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        let generation = self.save.generation;
        let revision = self.search.diagram.request.generation;
        self.save.busy = true;
        self.status = "Preparing the schema changes…".into();
        cx.spawn(async move |this, cx| {
            let target = table.clone();
            let session = live.clone();
            let result = cx
                .background_executor()
                .spawn(async move { plan(&session, &target) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.search.diagram.request.generation != revision
                    || !matches!(this.tabs.current(), Tab::Diagram)
                    || this.live.as_ref().is_none_or(|current| !Arc::ptr_eq(current, &live))
                {
                    this.save.busy = false;
                    this.status = "The diagram changed while preparing the review".into();
                } else {
                    let (plan, columns) = match result {
                        Ok((plan, columns)) => (Ok(plan), columns),
                        Err(error) => (Err(error), None),
                    };
                    this.receive_plan(plan, labels, (generation, table), Vec::new());
                    if let Some(pending) = &mut this.save.pending {
                        pending.diagram = Some((revision, columns));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn receive_plan(
        &mut self,
        planned: Result<PreparedSave, String>,
        labels: ReviewLabels,
        (generation, table): (u64, bookie_core::TableInfo),
        checks: Vec<CellCheck>,
    ) {
        self.save.busy = false;
        if self.save.generation != generation {
            self.status = "The edits changed while the changes were prepared; try again".into();
            return;
        }
        match planned {
            Ok((plan, _)) if plan.statements.is_empty() => {
                self.cell_edit.edits.clear();
                self.reset_edit_history();
                self.status = "Nothing changed, so there is nothing to save".into();
            }
            Ok((plan, relationship)) => {
                let rows = plan.statements.len().to_string();
                self.status = labels.review;
                self.save.pending = Some(PendingSave {
                    relationship,
                    diagram: None,
                    plan,
                    title: labels.title.replace("{n}", &rows),
                    done: labels.done.replace("{n}", &rows),
                    confirm: labels.confirm,
                    destructive: labels.destructive,
                    note: labels.note,
                    reconnect: labels.reconnect,
                    generation,
                    table,
                    checks,
                });
            }
            Err(message) => self.status = format!("Cannot save: {message}"),
        }
    }

    fn show_banner(&mut self, text: String, is_error: bool, cx: &mut Context<Self>) {
        let kind = if is_error {
            crate::notifications::NoticeKind::Error
        } else {
            crate::notifications::NoticeKind::Success
        };
        self.notices.push(text.clone(), kind);
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

    pub(crate) fn report_save_check(&mut self, checks: &[CellCheck]) {
        if checks.is_empty() {
            return;
        }
        let keys = key_columns(self.source.columns());
        match verify(self.source.as_ref(), &keys, checks).warning() {
            Some(warning) => {
                self.status = format!("{}. {warning}", self.status);
                self.save.banner_serial += 1;
                self.save.banner = Some((self.status.clone(), true));
                self.notices
                    .push(self.status.clone(), crate::notifications::NoticeKind::Error);
            }
            None => self.status = format!("{}, verified by re-reading the rows", self.status),
        }
    }

    pub(crate) fn cancel_save(&mut self, cx: &mut Context<Self>) {
        if self.save.pending.take().is_some() {
            self.status = "Canceled the save; the edits are still pending".into();
            cx.notify();
        }
    }

    pub(crate) fn confirm_save(&mut self, cx: &mut Context<Self>) {
        if self.transaction_open() {
            self.status = TRANSACTION_BLOCKS_SAVE.into();
            cx.notify();
            return;
        }
        let (Some(pending), Some(live)) = (self.save.pending.take(), self.live.clone()) else {
            return;
        };
        let Some(connection_id) = live.writable else {
            return;
        };
        let target_current = match &pending.diagram {
            Some((revision, _)) => {
                matches!(self.tabs.current(), Tab::Diagram) && *revision == self.search.diagram.request.generation
            }
            None => {
                pending.table == self.catalog.grid_table
                    && matches!(self.tabs.current(), Tab::Table(table) if *table == pending.table)
            }
        };
        if self.save.busy || pending.generation != self.save.generation || !target_current {
            self.status = format!(
                "The table or the edits changed since the review; {}",
                self.save_hint("review again")
            );
            cx.notify();
            return;
        }
        self.save.busy = true;
        self.save.reconnect_after = pending.reconnect.clone();
        self.save.checks = pending.checks;
        self.status = "Saving…".into();
        let write = self
            .save
            .write
            .as_ref()
            .filter(|(id, _)| *id == connection_id)
            .map(|(_, write)| write.clone());
        let approvals = self.approval.tx.clone();
        let opened_with = live.stamp.clone();
        let done = pending.done;
        self.save.refresh_diagram = pending.diagram.as_ref().map(|(_, columns)| {
            if columns.is_none() && pending.relationship.is_none() {
                pending.table.schema.clone()
            } else {
                self.search.diagram.scope.clone()
            }
        });
        let diagram_columns = pending.diagram.and_then(|(_, columns)| columns);
        let table = pending.table;
        let relationship = pending.relationship;
        let (statements, expect_one) = (pending.plan.statements, pending.plan.expect_one);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let write = match write.filter(|write| write.is_current().unwrap_or(false)) {
                        Some(write) => write,
                        None => Arc::new(WriteSession::open_saved(
                            connection_id,
                            approvals,
                            opened_with.as_deref(),
                        )?),
                    };
                    let precheck = relationship
                        .map_or(Ok(()), |review| review.verify(&live))
                        .and_then(|()| {
                            if let Some(columns) = diagram_columns
                                && live.diagram_columns(&table)? != columns
                            {
                                return Err(
                                    "The table changed since review. Refresh the diagram and review again".into()
                                );
                            }
                            Ok(())
                        });
                    let applied = match precheck {
                        Ok(()) => write.apply(&statements, &expect_one, &tokio_util::sync::CancellationToken::new()),
                        Err(message) => Err(SaveFailure {
                            message,
                            outcome_unknown: false,
                        }),
                    };
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

    fn reload_after_save_or_reconnect(
        &mut self,
        connection_id: uuid::Uuid,
        text: String,
        reconnect: Option<bookie_core::TableInfo>,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_ref() {
            live.forget_columns();
        }
        if let Some(table) = reconnect {
            self.status = text;
            let diagram = self.save.refresh_diagram.take();
            self.search.diagram.ddl = None;
            self.save.diagram_after_reconnect = diagram.filter(|_| matches!(self.tabs.current(), Tab::Diagram));
            let origin = self
                .search
                .origin
                .clone()
                .unwrap_or_else(|| bookie_gpui::live::Origin::Saved(connection_id.to_string()));
            self.connect_preferring(origin, Some(table), cx);
        } else if let Some(scope) = self.save.refresh_diagram.take() {
            self.status = text;
            self.search.diagram.ddl = None;
            if matches!(self.tabs.current(), Tab::Diagram) {
                self.load_schema_diagram(scope, cx);
            } else {
                self.reset_diagram();
            }
        } else {
            self.reload_after_save(text, cx);
        }
    }

    pub(crate) fn finish_save(
        &mut self,
        connection_id: uuid::Uuid,
        done: String,
        result: SaveOutcome,
        cx: &mut Context<Self>,
    ) {
        let reconnect = self.save.reconnect_after.take();
        let (write, applied) = match result {
            Ok(done) => done,
            Err(message) => {
                self.save.busy = false;
                self.save.checks.clear();
                self.save.refresh_diagram = None;
                self.status = format!("Could not open a connection for saving: {message}");
                self.show_banner(self.status.clone(), true, cx);
                return;
            }
        };
        self.save.write = applied.is_ok().then(|| (connection_id, write));
        match applied {
            Ok(_) => {
                self.save.busy = false;
                self.close_insert_form();
                self.close_import_bar();
                self.close_structure_bar();
                self.show_banner(done.clone(), false, cx);
                self.reload_after_save_or_reconnect(connection_id, done, reconnect, cx);
            }
            Err(failure) if failure.outcome_unknown => {
                let text = format!(
                    "The save may or may not have been applied ({}). The table was reloaded; check the data before saving again. If writes stay blocked, use Resolve outcome in its notification.",
                    failure.message
                );
                self.save.busy = false;
                self.save.banner_serial += 1;
                self.save.banner = Some((text.clone(), true));
                self.push_notice(text.clone(), crate::notifications::NoticeKind::UnknownOutcome, cx);
                self.reload_after_save_or_reconnect(connection_id, text, reconnect, cx);
            }
            Err(failure) => {
                self.save.busy = false;
                self.save.checks.clear();
                self.save.refresh_diagram = None;
                self.status = format!("Could not save: {}", with_resolve_hint(&failure.message));
                self.show_banner(self.status.clone(), true, cx);
            }
        }
    }

    pub(crate) fn pending_marker(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let count = self.cell_edit.edits.len();
        if count == 0 || self.compare.is_some() || self.save.busy || self.save.pending.is_some() {
            return None;
        }
        let can_save = self.can_save_edits();
        let (label, explanation) = if can_save {
            (
                format!("{count} pending"),
                "Changes not saved to the database yet".to_owned(),
            )
        } else {
            (
                format!("{count} view only"),
                self.view_edit_refusal()
                    .unwrap_or_else(|| "Edits in this view only; nothing is saved".into()),
            )
        };
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_1()
                .debug_selector(|| "pending-edits-marker".into())
                .child(
                    div()
                        .id("pending-edits-count")
                        .whitespace_nowrap()
                        .text_color(rgb(theme.warning))
                        .tooltip(tip(explanation))
                        .child(label),
                )
                .when(can_save, |marker| {
                    marker.child(
                        compact_button(ButtonKind::Ghost, "save-pending-edits", "Save", theme)
                            .debug_selector(|| "save-pending-edits".into())
                            .tooltip("Review the statements, then save or cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.request_save(cx))),
                    )
                })
                .child(
                    compact_button(ButtonKind::Ghost, "discard-pending-edits", "Discard", theme)
                        .debug_selector(|| "discard-pending-edits".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.discard_view_edits();
                            cx.notify();
                        })),
                ),
        )
    }

    pub(crate) fn save_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let pending = self.save.pending.as_ref()?;
        Some(
            task_bar_frame(theme)
                .flex_col()
                .items_stretch()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(task_title(pending.title.clone()))
                        .child(
                            button(
                                confirm_kind(pending.destructive),
                                "confirm-save",
                                pending.confirm,
                                theme,
                            )
                            .tooltip("Run these statements in one transaction")
                            .on_click(cx.listener(|this, _, _, cx| this.confirm_save(cx))),
                        )
                        .child(
                            button(ButtonKind::Ghost, "cancel-save", "Cancel", theme)
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_save(cx))),
                        ),
                )
                .children(pending.note.map(|note| {
                    div()
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child(note)
                }))
                .child(sql_preview(
                    "save-preview",
                    pending.plan.preview.clone(),
                    size::SAVE_PREVIEW_MAX_HEIGHT,
                    theme,
                ))
                .children(preview_overflows(&pending.plan.preview).then(|| {
                    div()
                        .debug_selector(|| "save-preview-more".into())
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child(format!(
                            "{} lines; scroll the preview for the rest",
                            pending.plan.preview.lines().count()
                        ))
                })),
        )
    }
}

pub(crate) fn confirm_kind(destructive: bool) -> ButtonKind {
    if destructive {
        ButtonKind::Destructive
    } else {
        ButtonKind::Primary
    }
}

fn preview_overflows(preview: &str) -> bool {
    preview.lines().count() as f32 * size::SAVE_PREVIEW_LINE_HEIGHT > size::SAVE_PREVIEW_MAX_HEIGHT
}

#[cfg(test)]
mod preview_tests {
    use super::preview_overflows;

    #[test]
    fn a_short_preview_needs_no_scroll_cue_and_a_long_one_does() {
        assert!(!preview_overflows("UPDATE t SET a = ? WHERE id = ?"));
        let long = (0..12).map(|n| format!("{n}: value")).collect::<Vec<_>>().join("\n");
        assert!(preview_overflows(&long));
    }
}

fn prepare_diagram_foreign_key(
    live: &LiveSession,
    source: &bookie_core::TableInfo,
    source_columns: Vec<bookie_core::ColumnInfo>,
    key: bookie_core::ForeignKeyInfo,
) -> Result<PreparedSave, String> {
    let target = bookie_core::TableInfo {
        schema: key.ref_schema.clone(),
        name: key.ref_table.clone(),
    };
    let target_columns = live.diagram_columns(&target)?;
    let target_spec = SaveTarget {
        driver_id: &live.driver_id,
        schema: source.schema.as_deref(),
        table: &source.name,
        columns: &source_columns,
    };
    let plan = bookie_app_services::structure_change_plan::plan_foreign_key(target_spec, key, &target_columns)
        .map_err(|error| error.to_string())?;
    Ok((
        CellSavePlan {
            statements: plan.statements,
            expect_one: Vec::new(),
            preview: plan.preview,
        },
        Some(ForeignKeyReview {
            source: source.clone(),
            target,
            source_columns,
            target_columns,
        }),
    ))
}

fn plan_structure(
    target: SaveTarget<'_>,
    kind: bookie_app_services::structure_change_plan::StructureChangeKind,
    text: &str,
    structure: &bookie_app_services::read_session::TableStructure,
) -> Result<CellSavePlan, String> {
    let plan = bookie_app_services::structure_change_plan::plan_structure_change(target, kind, text, structure)
        .map_err(|error| error.to_string())?;
    Ok(CellSavePlan {
        statements: plan.statements,
        expect_one: Vec::new(),
        preview: plan.preview,
    })
}

pub(crate) fn with_resolve_hint(message: &str) -> String {
    if message.contains("governed writes are disabled") {
        format!(
            "{message}. If a write's result is unknown, decide it with 'Resolve unknown write outcome…' in the command palette"
        )
    } else {
        message.to_string()
    }
}

fn plan_file_import(
    target: SaveTarget<'_>,
    driver_id: &str,
    file: &std::path::Path,
    choices: &bookie_app_services::csv_import_plan::ImportChoices,
) -> Result<CellSavePlan, String> {
    let capabilities = drivers_registry::default_registry()
        .get(driver_id)
        .map(|driver| driver.capabilities())
        .unwrap_or_default();
    let file = bookie_core::user_path::expand_tilde(file);
    let plan = plan_csv_import(target, capabilities, &file, crate::import_ui::IMPORT_ROW_CAP, choices)
        .map_err(|error| error.to_string())?;
    let cut = if plan.truncated {
        format!(" (the file has more; only the first {} are imported)", plan.rows)
    } else {
        String::new()
    };
    let filled = if plan.filled_by_database.is_empty() {
        String::new()
    } else {
        format!(
            "\nNot imported, the database fills them: {}",
            plan.filled_by_database.join(", ")
        )
    };
    let too_big = if plan.approvable {
        String::new()
    } else {
        "\nToo large to approve: if this connection asks for approval, it will be refused. Import fewer rows.\n"
            .to_string()
    };
    let ignored = if plan.ignored_fields.is_empty() {
        String::new()
    } else {
        format!(
            "\nIgnored fields with no matching column: {}",
            plan.ignored_fields.join(", ")
        )
    };
    let preview = format!(
        "{} row(s) from the file{cut}\nColumns: {}{ignored}{filled}{too_big}\n\n{}",
        plan.rows,
        plan.imported_columns.join(", "),
        plan.preview
    );
    Ok(CellSavePlan {
        statements: plan.statements,
        expect_one: Vec::new(),
        preview,
    })
}
