mod explanation;
mod mutations;

use std::sync::Arc;

use bookie_app_services::result_editability::{
    EditTarget, ResultLock, ResultRefusal, SourceTable, editable_target, grammar_for_driver, owned_result_columns,
    source_table,
};
use bookie_core::{ColumnInfo, QueryResult, TableInfo};
use relm4::ComponentSender;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::prelude::*;
use uuid::Uuid;

use super::outcomes::{clear_box, result_view_key};
use super::{SqlEditor, SqlEditorInput, SqlEditorOutput, StatementIdentity, StatementOutcome, StatementOutcomeKind};
use crate::services::change_tracker::WorkspaceChangeTrackers;
use crate::ui::grid::{TabGridContext, build_column_view};
use explanation::refusal_explanation;

#[derive(Debug, Clone)]
enum ResultEdit {
    Locked(ResultRefusal),
    Checking,
    Editable,
}

struct PendingBar {
    revealer: gtk::Revealer,
    pending_label: gtk::Label,
    save_button: gtk::Button,
    discard_button: gtk::Button,
    preview_button: gtk::Button,
}

pub(super) struct ResultEditing {
    pub(super) change_trackers: WorkspaceChangeTrackers,
    pub(super) tab_id: Uuid,
    run: u64,
    state: Option<ResultEdit>,
    result: Option<Arc<QueryResult>>,
    columns: Vec<ColumnInfo>,
    table: Option<TableInfo>,
    sql: String,
    driver_id: String,
    results_content: gtk::Box,
    refusal_banner: adw::Banner,
    pending: PendingBar,
    selection: Option<gtk::MultiSelection>,
    column_view: Option<gtk::ColumnView>,
    save_in_flight: Option<Uuid>,
    rereading: bool,
    quiet_run: bool,
    was_dirty: bool,
    deferred_run: Option<DeferredRun>,
    identity: StatementIdentity,
}

#[derive(Debug, Clone)]
pub(super) struct DeferredRun {
    pub(super) sql: String,
    pub(super) anchor: Option<super::error_navigation::SourceAnchor>,
    pub(super) single_statement: bool,
}

impl ResultEditing {
    pub(super) fn build(change_trackers: WorkspaceChangeTrackers, tab_id: Uuid, holder: &gtk::Box) -> Self {
        change_trackers.open_tab(tab_id);
        let results_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        results_content.set_vexpand(true);
        results_content.set_hexpand(true);
        let refusal_banner = adw::Banner::builder().revealed(false).use_markup(false).build();
        let pending = build_pending_bar();
        holder.append(&refusal_banner);
        holder.append(&results_content);
        holder.append(&pending.revealer);
        Self {
            change_trackers,
            tab_id,
            run: 0,
            state: None,
            result: None,
            columns: Vec::new(),
            table: None,
            sql: String::new(),
            driver_id: String::new(),
            results_content,
            refusal_banner,
            pending,
            selection: None,
            column_view: None,
            save_in_flight: None,
            rereading: false,
            quiet_run: false,
            was_dirty: false,
            deferred_run: None,
            identity: StatementIdentity::default(),
        }
    }

    pub(super) fn wire_actions(&self, sender: &ComponentSender<SqlEditor>) {
        let discard = sender.clone();
        self.pending
            .discard_button
            .connect_clicked(move |_| discard.input(SqlEditorInput::DiscardResultEdits));
        let save = sender.clone();
        self.pending
            .save_button
            .connect_clicked(move |_| save.input(SqlEditorInput::CommitResultEdits));
        let preview = sender.clone();
        self.pending
            .preview_button
            .connect_clicked(move |_| preview.input(SqlEditorInput::PreviewResultEdits));
    }
}

fn build_pending_bar() -> PendingBar {
    let pending_label = gtk::Label::new(None);
    pending_label.add_css_class("dim-label");
    pending_label.add_css_class("caption");
    let discard_button = gtk::Button::builder()
        .label(crate::tr!("Discard"))
        .tooltip_text(crate::tr!("Discard all pending edits"))
        .build();
    let save_button = gtk::Button::builder()
        .label(crate::tr!("Save"))
        .tooltip_text(crate::tr!("Save pending edits (Ctrl+S)"))
        .build();
    save_button.add_css_class("suggested-action");
    let preview_button = gtk::Button::builder()
        .label(crate::tr!("Preview SQL"))
        .tooltip_text(crate::tr!("Show the statements Save would run"))
        .build();
    let bar = gtk::ActionBar::new();
    bar.pack_start(&pending_label);
    bar.pack_end(&save_button);
    bar.pack_end(&discard_button);
    bar.pack_end(&preview_button);
    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .transition_duration(150)
        .reveal_child(false)
        .child(&bar)
        .build();
    PendingBar {
        revealer,
        pending_label,
        save_button,
        discard_button,
        preview_button,
    }
}

impl SqlEditor {
    pub(super) fn results_surface(&self) -> &gtk::Box {
        self.result_editing
            .as_ref()
            .map(|editing| &editing.results_content)
            .unwrap_or(&self.results_holder)
    }

    pub(super) fn clear_result_surface(&self) {
        clear_box(self.results_surface());
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        editing.refusal_banner.set_revealed(false);
        editing.refusal_banner.set_title("");
        editing.pending.revealer.set_reveal_child(false);
    }

    pub(super) fn forget_result_edits(&mut self) {
        let Some(editing) = self.result_editing.as_mut() else {
            return;
        };
        editing.run = editing.run.wrapping_add(1);
        editing.state = None;
        editing.result = None;
        editing.columns.clear();
        editing.table = None;
        editing.sql.clear();
        editing.selection = None;
        editing.column_view = None;
        editing.save_in_flight = None;
        editing
            .change_trackers
            .with_tab(editing.tab_id, |tracker| tracker.clear());
        self.refresh_result_pending_bar(0, None);
    }

    pub(super) fn starts_quiet_run(&mut self) {
        if let Some(editing) = self.result_editing.as_mut() {
            editing.quiet_run = editing.rereading;
        }
    }

    pub(super) fn ends_quiet_run(&mut self) -> bool {
        self.result_editing
            .as_mut()
            .map(|editing| std::mem::replace(&mut editing.quiet_run, false))
            .unwrap_or(false)
    }

    pub(super) fn has_pending_result_edits(&self) -> bool {
        let Some(editing) = self.result_editing.as_ref() else {
            return false;
        };
        editing
            .change_trackers
            .with_tab_ref(editing.tab_id, |tracker| tracker.has_pending())
            .unwrap_or(false)
    }

    pub(super) fn result_save_in_flight(&self) -> bool {
        self.result_editing
            .as_ref()
            .is_some_and(|editing| editing.save_in_flight.is_some())
    }

    pub(super) fn decide_result_edits(&mut self, outcomes: &[StatementOutcome], sender: &ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_mut() else {
            return;
        };
        editing.run = editing.run.wrapping_add(1);
        editing.state = None;
        editing.result = None;
        editing.columns.clear();
        editing.table = None;
        editing.selection = None;
        editing.column_view = None;
        editing.identity = outcomes.first().map(|outcome| outcome.identity).unwrap_or_default();
        editing
            .change_trackers
            .with_tab(editing.tab_id, |tracker| tracker.clear());
        self.refresh_result_pending_bar(0, Some(sender));
        if let Some(refusal) = common_refusal(self, outcomes) {
            self.lock_result_edits(ResultEdit::Locked(refusal));
            return;
        }
        let Some(outcome) = outcomes.first() else {
            return;
        };
        let StatementOutcomeKind::Rows(sets) = &outcome.kind else {
            self.lock_result_edits(ResultEdit::Locked(stream_or_other_refusal(&outcome.kind)));
            return;
        };
        if sets.len() != 1 {
            self.lock_result_edits(ResultEdit::Locked(ResultRefusal::MultiResult));
            return;
        }
        let Some(result) = sets.first().cloned() else {
            return;
        };
        let driver_id = self.metadata().map(|meta| meta.driver_id).unwrap_or_default();
        let Some(grammar) = grammar_for_driver(&driver_id) else {
            self.lock_result_edits(ResultEdit::Locked(ResultRefusal::NotOneSelect));
            return;
        };
        let source = match source_table(&outcome.sql, grammar) {
            Ok(source) => source,
            Err(lock) => {
                self.lock_result_edits(ResultEdit::Locked(ResultLock::from(lock).into()));
                return;
            }
        };
        let run = {
            let Some(editing) = self.result_editing.as_mut() else {
                return;
            };
            editing.sql = outcome.sql.clone();
            editing.driver_id = driver_id;
            editing.result = Some(result);
            editing.state = Some(ResultEdit::Checking);
            editing.run
        };
        self.show_refusal(ResultRefusal::Checking);
        self.fetch_edit_columns(run, source, sender);
    }

    fn lock_result_edits(&mut self, state: ResultEdit) {
        if let ResultEdit::Locked(refusal) = &state {
            self.show_refusal(*refusal);
        }
        if let Some(editing) = self.result_editing.as_mut() {
            editing.state = Some(state);
        }
    }

    fn show_refusal(&self, refusal: ResultRefusal) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        editing.refusal_banner.set_title(&refusal_explanation(refusal));
        editing.refusal_banner.set_revealed(true);
    }

    fn hide_refusal(&self) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        editing.refusal_banner.set_revealed(false);
        editing.refusal_banner.set_title("");
    }

    fn fetch_edit_columns(&self, run: u64, source: SourceTable, sender: &ComponentSender<Self>) {
        let table = source.table.clone();
        let fail = || {
            sender.input(SqlEditorInput::EditColumnsFetched {
                run,
                source: source.clone(),
                columns: Err(()),
            });
        };
        let Some(connection_id) = self.connection_id else {
            fail();
            return;
        };
        let Some(conn) = self.database.get(connection_id) else {
            fail();
            return;
        };
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let schema = table.schema.clone();
        let name = table.name.clone();
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    let columns = conn
                        .fetch_columns_controlled(schema.as_deref(), &name, &control)
                        .await
                        .map_err(|_| ());
                    sender_clone.input(SqlEditorInput::EditColumnsFetched { run, source, columns });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_edit_columns_fetched(
        &mut self,
        run: u64,
        source: SourceTable,
        columns: Result<Vec<ColumnInfo>, ()>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        if editing.run != run {
            return;
        }
        let Some(result) = editing.result.clone() else {
            return;
        };
        let catalog = match columns {
            Ok(columns) => columns,
            Err(()) => {
                self.lock_result_edits(ResultEdit::Locked(ResultRefusal::ColumnsUnread));
                return;
            }
        };
        let target = match editable_target(&source, &result.columns, &catalog) {
            Ok(target) => target,
            Err(lock) => {
                self.lock_result_edits(ResultEdit::Locked(lock.into()));
                return;
            }
        };
        let owned = owned_result_columns(&result.columns, &target);
        let mut editable = (*result).clone();
        editable.columns = owned.clone();
        let editable = Arc::new(editable);
        {
            let Some(editing) = self.result_editing.as_mut() else {
                return;
            };
            editing.columns = owned;
            editing.table = Some(target.table.clone());
            editing.result = Some(editable.clone());
            editing.state = Some(ResultEdit::Editable);
            editing
                .change_trackers
                .with_tab(editing.tab_id, |tracker| tracker.clear());
        }
        self.hide_refusal();
        self.rebuild_editable_grid(&editable, &target);
        let _ = sender;
    }

    fn rebuild_editable_grid(&mut self, result: &Arc<QueryResult>, target: &EditTarget) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        clear_box(&editing.results_content);
        if result.columns.is_empty() {
            let empty = adw::StatusPage::builder()
                .title(crate::tr!("No rows returned"))
                .icon_name("emblem-default-symbolic")
                .vexpand(true)
                .build();
            editing.results_content.append(&empty);
            return;
        }
        let (column_view, selection, holder) = self.build_editable_column_view(result, target, editing);
        editing.results_content.append(&holder);
        let Some(editing) = self.result_editing.as_mut() else {
            return;
        };
        editing.column_view = Some(column_view);
        editing.selection = Some(selection);
    }

    fn build_editable_column_view(
        &self,
        result: &Arc<QueryResult>,
        target: &EditTarget,
        editing: &ResultEditing,
    ) -> (gtk::ColumnView, gtk::MultiSelection, gtk::Box) {
        let pk_col_indices: Vec<usize> = editing
            .columns
            .iter()
            .enumerate()
            .filter(|(_, column)| column.primary_key)
            .map(|(index, _)| index)
            .collect();
        let settings_key = result_view_key(editing.identity, 0);
        let local_result_view = crate::ui::grid::local_filter::LocalResultView::new(
            result.clone(),
            self.result_view_settings.clone(),
            settings_key,
        );
        let (column_view, selection) = build_column_view(
            result,
            &editing.columns,
            &target.table.name,
            None,
            crate::ui::grid::GridSenders {
                edit: Some(self.grid_sender.clone()),
                menu: Some(self.grid_sender.clone()),
                sort: None,
            },
            crate::ui::grid::GridEnv {
                connection_id: self.connection_id,
                tab_ctx: TabGridContext {
                    change_trackers: Some(editing.change_trackers.clone()),
                    tab_id: Some(editing.tab_id),
                    pk_col_indices,
                    preview_result_values: true,
                    local_result_sort: true,
                    local_result_view: Some(local_result_view.clone()),
                    whole_result_filter: false,
                    changed_cells: self.result_view_settings.changed_cells(settings_key),
                    sortable_result_columns: crate::ui::grid::sortable_result_columns(result),
                    row_mutations_allowed: true,
                    ..Default::default()
                },
                column_widths: None,
                database: self.database.clone(),
            },
        );
        local_result_view.bind_column_view(&column_view);
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.append(&local_result_view.controls());
        holder.append(&crate::ui::grid::wrap_grid_scrolled(&column_view));
        (column_view, selection, holder)
    }

    pub(super) fn refresh_result_pending_bar(&mut self, count: usize, sender: Option<&ComponentSender<Self>>) {
        let Some(editing) = self.result_editing.as_mut() else {
            return;
        };
        let visible = count > 0;
        if visible {
            let label =
                crate::trn!("{n} unsaved change", "{n} unsaved changes", count).replace("{n}", &count.to_string());
            editing.pending.pending_label.set_label(&label);
        }
        editing.pending.revealer.set_reveal_child(visible);
        let dirty = count > 0;
        if dirty != editing.was_dirty {
            editing.was_dirty = dirty;
            if let Some(sender) = sender {
                let _ = sender.output(SqlEditorOutput::DirtyChanged(dirty));
            }
        }
    }

    pub(super) fn gate_run_for_pending_edits(&mut self, deferred: DeferredRun, sender: &ComponentSender<Self>) -> bool {
        if self.result_save_in_flight() {
            self.status
                .set_label(&crate::tr!("Wait for the changes to finish saving"));
            return false;
        }
        if !self.has_pending_result_edits() {
            if let Some(editing) = self.result_editing.as_mut() {
                editing.deferred_run = None;
            }
            return true;
        }
        if let Some(editing) = self.result_editing.as_mut() {
            editing.deferred_run = Some(deferred);
        }
        let dialog = adw::AlertDialog::new(
            Some(&crate::tr!("Discard pending changes?")),
            Some(&crate::tr!(
                "Running again replaces these results and drops every unsaved edit on this tab."
            )),
        );
        dialog.add_response("cancel", &crate::tr!("Cancel"));
        dialog.add_response("discard", &crate::tr!("Discard and run"));
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let input = sender.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "discard" {
                input.input(SqlEditorInput::ConfirmDiscardAndRun);
            }
        });
        if let Some(root) = self.source_view.root() {
            dialog.present(Some(&root));
        }
        false
    }

    pub(super) fn on_confirm_discard_and_run(&mut self, sender: ComponentSender<Self>) {
        self.forget_result_edits();
        let deferred = self
            .result_editing
            .as_mut()
            .and_then(|editing| editing.deferred_run.take());
        let Some(deferred) = deferred else {
            return;
        };
        self.begin_run(deferred.sql, deferred.anchor, deferred.single_statement, sender);
    }
}

fn common_refusal(editor: &SqlEditor, outcomes: &[StatementOutcome]) -> Option<ResultRefusal> {
    let metadata = editor.metadata()?;
    if metadata.read_only {
        return Some(ResultRefusal::ReadOnlySession);
    }
    if editor.session_transaction_open() {
        return Some(ResultRefusal::OpenTransaction);
    }
    if outcomes.len() > 1 {
        return Some(ResultRefusal::MultiResult);
    }
    None
}

fn stream_or_other_refusal(kind: &StatementOutcomeKind) -> ResultRefusal {
    match kind {
        StatementOutcomeKind::Streamed(_) => ResultRefusal::NotOneSelect,
        StatementOutcomeKind::Rows(_) => ResultRefusal::MultiResult,
        StatementOutcomeKind::Error(_) | StatementOutcomeKind::NotRun => ResultRefusal::NotOneSelect,
    }
}
