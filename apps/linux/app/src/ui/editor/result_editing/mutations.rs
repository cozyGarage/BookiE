use bookie_core::Value;
use relm4::ComponentSender;
use relm4::gtk::prelude::*;
use uuid::Uuid;

use super::super::{SqlEditor, SqlEditorOutput};
use super::ResultEdit;
use crate::services::change_tracker::{RowKey, StatementSource};
use crate::ui::grid::GridMsg;

fn should_block_mutation_during_save(message: &GridMsg, save_in_flight: bool) -> bool {
    save_in_flight
        && matches!(
            message,
            GridMsg::CellEdited { .. }
                | GridMsg::CellEditedValue { .. }
                | GridMsg::SetCells(_)
                | GridMsg::SetCellsConfirmed { .. }
                | GridMsg::SetCellNull { .. }
                | GridMsg::DeleteRowAt { .. }
                | GridMsg::InsertRow
                | GridMsg::DuplicateRow { .. }
        )
}

impl SqlEditor {
    pub(in crate::ui::editor) fn on_editable_grid_message(
        &mut self,
        message: GridMsg,
        sender: &ComponentSender<Self>,
    ) -> Option<GridMsg> {
        if let GridMsg::ReplaceBatch {
            report,
            generation,
            cancel,
            reply,
        } = message
        {
            self.handle_result_replace_batch(report, generation, cancel, reply);
            return None;
        }
        if !self.result_is_editable() {
            return Some(message);
        }
        if should_block_mutation_during_save(&message, self.result_save_in_flight()) {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "Saving changes. Edit again when the save finishes."
            )));
            return None;
        }
        self.dispatch_editable_grid_message(message, sender)
    }

    fn dispatch_editable_grid_message(&mut self, message: GridMsg, sender: &ComponentSender<Self>) -> Option<GridMsg> {
        match message {
            GridMsg::SetCells(_) | GridMsg::SetCellsConfirmed { .. } => {
                self.dispatch_set_cells(message, sender.clone());
                None
            }
            GridMsg::CellEdited {
                row_position,
                col_index,
                new_value,
                row_key,
            } => self.dispatch_cell_edited(row_position, col_index, new_value, row_key, sender),
            GridMsg::CellEditedValue {
                row_position,
                col_index,
                value,
                row_key,
                draft_id,
            } => {
                if self.result_row_object(row_position).and_then(|row| row.draft_id()) == draft_id {
                    self.handle_result_cell_value_edited(row_position, col_index, value, row_key, sender);
                }
                None
            }
            GridMsg::SetCellNull {
                row_position,
                col_index,
                row_key,
                draft_id,
            } => {
                self.handle_result_set_null(row_position, col_index, row_key, draft_id, sender);
                None
            }
            GridMsg::InsertRow => {
                self.handle_result_insert_row(sender);
                None
            }
            GridMsg::DuplicateRow {
                row_position,
                row_key,
                draft_id,
            } => {
                if self.result_row_object(row_position).and_then(|row| row.draft_id()) == draft_id
                    && (draft_id.is_some() || !row_key.is_empty())
                    && !self.reject_stale_result_edit(row_position, &row_key, sender)
                {
                    self.handle_result_duplicate_row(row_position, sender);
                }
                None
            }
            GridMsg::DeleteRowAt {
                row_position,
                row_key,
                draft_id,
            } => {
                self.handle_result_delete_row(row_position, row_key, draft_id, sender);
                None
            }
            other => Some(other),
        }
    }

    fn dispatch_cell_edited(
        &mut self,
        row_position: u32,
        col_index: usize,
        new_value: String,
        row_key: Vec<Value>,
        sender: &ComponentSender<Self>,
    ) -> Option<GridMsg> {
        self.handle_result_cell_edited(row_position, col_index, new_value, row_key, sender);
        None
    }

    fn handle_result_replace_batch(
        &mut self,
        report: bookie_app_services::grid_replace::ReplaceReport,
        generation: u64,
        cancel: tokio_util::sync::CancellationToken,
        reply: tokio::sync::oneshot::Sender<
            Result<
                bookie_app_services::grid_replace::ReplaceAcknowledgement,
                bookie_app_services::grid_replace::ReplaceApplyError,
            >,
        >,
    ) {
        use bookie_app_services::grid_replace::{ReplaceAcknowledgement, ReplaceApplyError};
        if cancel.is_cancelled() {
            let _ = reply.send(Err(ReplaceApplyError::Cancelled));
            return;
        }
        if !self.result_is_editable() {
            let _ = reply.send(Err(ReplaceApplyError::NotEditable));
            return;
        }
        if self
            .result_editing
            .as_ref()
            .is_none_or(|editing| editing.save_in_flight.is_some())
        {
            let _ = reply.send(Err(ReplaceApplyError::Saving));
            return;
        }
        if self
            .result_list_store()
            .is_none_or(|store| store.generation() != generation)
        {
            let _ = reply.send(Err(ReplaceApplyError::Stale));
            return;
        }
        let prepared = match self.prepare_result_replace(&report) {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let edits = prepared
            .edits
            .into_iter()
            .map(|edit| RowKey::from_pk_values(&edit.row_key).map(|key| (key, edit.column, edit.original, edit.value)))
            .collect::<Option<Vec<_>>>();
        let Some(edits) = edits else {
            let _ = reply.send(Err(ReplaceApplyError::Stale));
            return;
        };
        let Some(editing) = self.result_editing.as_ref() else {
            let _ = reply.send(Err(ReplaceApplyError::NotEditable));
            return;
        };
        if cancel.is_cancelled() {
            let _ = reply.send(Err(ReplaceApplyError::Cancelled));
            return;
        }
        let tracker = editing
            .change_trackers
            .with_tab(editing.tab_id, |tracker| tracker.track_cell_edits(edits));
        if tracker.is_none() {
            let _ = reply.send(Err(ReplaceApplyError::Stale));
            return;
        }
        let _ = reply.send(Ok(ReplaceAcknowledgement {
            replacements: prepared.replacements,
            skipped: prepared.skipped,
        }));
    }

    fn prepare_result_replace(
        &self,
        report: &bookie_app_services::grid_replace::ReplaceReport,
    ) -> Result<
        bookie_app_services::grid_replace::PreparedReplaceBatch,
        bookie_app_services::grid_replace::ReplaceApplyError,
    > {
        use bookie_app_services::grid_replace::{ReplaceApplyError, prepare_replace_plan};
        let Some(editing) = self.result_editing.as_ref() else {
            return Err(ReplaceApplyError::NotEditable);
        };
        let pk_indices = self.result_pk_indices();
        prepare_replace_plan(
            report,
            |position, edits| {
                let row = self.result_row_object(position)?;
                let key = pk_indices
                    .iter()
                    .map(|index| row.cell_value_in_row(*index))
                    .collect::<Option<Vec<_>>>()?;
                let cells = edits
                    .iter()
                    .map(|edit| row.cell_value_in_row(edit.column))
                    .collect::<Option<Vec<_>>>()?;
                Some((key, cells))
            },
            |column_index, original, new_value| {
                let Some(column) = editing.columns.get(column_index) else {
                    return Err(ReplaceApplyError::NotEditable);
                };
                if !crate::ui::grid::cell_allows_inline_edit(column, original) {
                    return Err(ReplaceApplyError::NotEditable);
                }
                crate::ui::browse_tab::parse_grid_cell_input(
                    new_value,
                    Some(column),
                    &editing.driver_id,
                    Some(original),
                )
                .map_err(|_| ReplaceApplyError::InvalidValue)
            },
        )
    }

    fn result_is_editable(&self) -> bool {
        self.result_editing
            .as_ref()
            .is_some_and(|editing| matches!(editing.state, Some(ResultEdit::Editable)))
    }

    fn result_list_store(&self) -> Option<crate::ui::row_store::RowStore> {
        let selection = self.result_editing.as_ref()?.selection.as_ref()?;
        selection.model()?.downcast::<crate::ui::row_store::RowStore>().ok()
    }

    #[cfg(test)]
    pub(in crate::ui::editor) fn editable_result_store_for_test(&self) -> Option<crate::ui::row_store::RowStore> {
        self.result_is_editable().then(|| self.result_list_store()).flatten()
    }

    fn result_row_object(&self, position: u32) -> Option<crate::ui::row_object::RowObject> {
        let model = self.result_editing.as_ref()?.selection.as_ref()?.model()?;
        model
            .item(position)?
            .downcast::<crate::ui::row_object::RowObject>()
            .ok()
    }

    fn refresh_result_row(&self, position: u32) {
        let Some(row) = self.result_row_object(position) else {
            return;
        };
        let Some(store) = self.result_list_store() else {
            return;
        };
        let Some(store_pos) = store.find(&row) else {
            return;
        };
        let replacement = row.clone_preserving_loading_state();
        store.splice(store_pos, 1, &[replacement]);
    }

    fn handle_result_cell_edited(
        &mut self,
        row_position: u32,
        col_index: usize,
        new_value: String,
        row_key: Vec<Value>,
        sender: &ComponentSender<Self>,
    ) {
        if self.reject_stale_result_edit(row_position, &row_key, sender) {
            return;
        }
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let allowed = editing
            .columns
            .get(col_index)
            .zip(self.result_row_object(row_position))
            .is_some_and(|(column, row)| crate::ui::grid::cell_allows_inline_edit(column, &row.cell_value(col_index)));
        if !allowed {
            self.refresh_result_row(row_position);
            return;
        }
        let col = editing.columns.get(col_index);
        let current = self
            .result_row_object(row_position)
            .map(|row| row.cell_value(col_index));
        let new =
            match crate::ui::browse_tab::parse_grid_cell_input(&new_value, col, &editing.driver_id, current.as_ref()) {
                Ok(value) => value,
                Err(message) => {
                    let _ = sender.output(SqlEditorOutput::ShowToast(message));
                    self.refresh_result_row(row_position);
                    return;
                }
            };
        self.stage_result_cell(row_position, col_index, new, sender);
    }

    fn handle_result_cell_value_edited(
        &mut self,
        row_position: u32,
        col_index: usize,
        value: Value,
        row_key: Vec<Value>,
        sender: &ComponentSender<Self>,
    ) {
        if self.reject_stale_result_edit(row_position, &row_key, sender) {
            return;
        }
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let row_obj = self.result_row_object(row_position);
        let allowed = editing
            .columns
            .get(col_index)
            .zip(row_obj.as_ref())
            .is_some_and(|(column, row)| {
                row.with_cell_value(col_index, |current| {
                    crate::ui::grid::full_value_editor::can_stage_full_value(column, current, &value)
                })
                .unwrap_or(false)
            });
        if !allowed {
            self.refresh_result_row(row_position);
            return;
        }
        self.stage_result_cell(row_position, col_index, value, sender);
    }

    fn handle_result_set_null(
        &mut self,
        row_position: u32,
        col_index: usize,
        row_key: Vec<Value>,
        draft_id: Option<u64>,
        sender: &ComponentSender<Self>,
    ) {
        if self.result_row_object(row_position).and_then(|row| row.draft_id()) != draft_id {
            return;
        }
        if self.reject_stale_result_edit(row_position, &row_key, sender) {
            return;
        }
        self.stage_result_cell(row_position, col_index, Value::Null, sender);
    }

    fn stage_result_cell(&mut self, row_position: u32, col_index: usize, new: Value, sender: &ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let Some(column) = editing.columns.get(col_index) else {
            return;
        };
        if !super::can_stage_result_value(column, &new) {
            self.refresh_result_row(row_position);
            return;
        }
        let tab_id = editing.tab_id;
        if let Some(row_obj) = self.result_row_object(row_position)
            && let Some(draft_id) = row_obj.draft_id()
        {
            editing.change_trackers.with_tab(tab_id, |tracker| {
                tracker.track_draft_cell_edit(draft_id, col_index, new.clone());
            });
            row_obj.set_cell(col_index, new);
            return;
        }
        let Some((key, cells)) = self.result_row_key_and_cells(row_position) else {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "This row can't be identified for editing"
            )));
            return;
        };
        let Some(original) = cells.get(col_index).cloned() else {
            self.refresh_result_row(row_position);
            return;
        };
        editing.change_trackers.with_tab(tab_id, |tracker| {
            tracker.track_cell_edit(key, col_index, original, new);
        });
    }

    fn result_pk_indices(&self) -> Vec<usize> {
        self.result_editing
            .as_ref()
            .map(|editing| {
                editing
                    .columns
                    .iter()
                    .enumerate()
                    .filter(|(_, column)| column.primary_key)
                    .map(|(index, _)| index)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn result_row_key_and_cells(&self, row_position: u32) -> Option<(RowKey, Vec<Value>)> {
        let row = self.result_row_object(row_position)?;
        if row.draft_id().is_some() {
            return None;
        }
        let cells = row.cells_clone();
        let pk_indices = self.result_pk_indices();
        let pk_values = pk_indices
            .iter()
            .map(|&index| cells.get(index).cloned())
            .collect::<Option<Vec<_>>>()?;
        let key = RowKey::from_pk_values(&pk_values)?;
        Some((key, cells))
    }

    fn reject_stale_result_edit(&self, row_position: u32, row_key: &[Value], sender: &ComponentSender<Self>) -> bool {
        let Some(editing) = self.result_editing.as_ref() else {
            return true;
        };
        if editing.save_in_flight.is_some() {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "Saving changes. Edit again when the save finishes."
            )));
            return true;
        }
        if row_key.is_empty() {
            return false;
        }
        let Some(expected) = RowKey::from_pk_values(row_key) else {
            return true;
        };
        let Some((current, _)) = self.result_row_key_and_cells(row_position) else {
            return self
                .result_row_object(row_position)
                .and_then(|row| row.draft_id())
                .is_none();
        };
        if current != expected {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "This row changed. Edit it again."
            )));
            self.refresh_result_row(row_position);
            return true;
        }
        let _ = editing;
        false
    }

    fn handle_result_insert_row(&mut self, sender: &ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        if editing.columns.is_empty() {
            return;
        }
        let defaults: Vec<Value> = editing.columns.iter().map(|_| Value::Null).collect();
        let tab_id = editing.tab_id;
        let key = editing
            .change_trackers
            .with_tab(tab_id, |tracker| tracker.track_insert(defaults.clone()));
        let Some(RowKey::Draft(draft_id)) = key else {
            return;
        };
        if let Some(store) = self.result_list_store() {
            let draft = crate::ui::row_object::RowObject::new_draft(draft_id, defaults);
            store.insert(0, &draft);
        }
        let _ = sender;
    }

    fn handle_result_duplicate_row(&mut self, row_position: u32, sender: &ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let Some(source) = self.result_row_object(row_position) else {
            return;
        };
        let cells = source.cells_clone();
        if cells.len() != editing.columns.len() {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "Cannot duplicate this row because its columns no longer match the result."
            )));
            return;
        }
        if cells.iter().any(|value| matches!(value, Value::Undecodable(_))) {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "Cannot duplicate this row because it contains an undecodable value."
            )));
            return;
        }
        let values: Vec<Value> = editing
            .columns
            .iter()
            .zip(cells)
            .map(|(column, value)| {
                if column.primary_key || column.is_generated || column.is_auto_increment {
                    Value::Null
                } else {
                    value
                }
            })
            .collect();
        let tab_id = editing.tab_id;
        let key = editing
            .change_trackers
            .with_tab(tab_id, |tracker| tracker.track_insert(values.clone()));
        let Some(RowKey::Draft(draft_id)) = key else {
            return;
        };
        if let Some(store) = self.result_list_store() {
            store.insert(0, &crate::ui::row_object::RowObject::new_draft(draft_id, values));
        }
    }

    fn handle_result_delete_row(
        &mut self,
        row_position: u32,
        row_key: Vec<Value>,
        draft_id: Option<u64>,
        sender: &ComponentSender<Self>,
    ) {
        if self.result_row_object(row_position).and_then(|row| row.draft_id()) != draft_id {
            return;
        }
        if self.reject_stale_result_edit(row_position, &row_key, sender) {
            return;
        }
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let tab_id = editing.tab_id;
        if let Some(draft_id) = self.result_row_object(row_position).and_then(|row| row.draft_id()) {
            editing.change_trackers.with_tab(tab_id, |tracker| {
                tracker.discard_draft(draft_id);
            });
            if let Some(store) = self.result_list_store()
                && let Some(row) = self.result_row_object(row_position)
                && let Some(pos) = store.find(&row)
            {
                store.remove(pos);
            }
            return;
        }
        let Some((key, cells)) = self.result_row_key_and_cells(row_position) else {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "This row can't be identified for editing"
            )));
            return;
        };
        editing.change_trackers.with_tab(tab_id, |tracker| {
            tracker.track_delete(key, cells);
        });
        self.refresh_result_row(row_position);
    }

    pub(in crate::ui::editor) fn on_result_pending_count_changed(
        &mut self,
        count: usize,
        sender: &ComponentSender<Self>,
    ) {
        self.refresh_result_pending_bar(count, Some(sender));
    }

    pub(in crate::ui::editor) fn on_result_changed_rows(&mut self, keys: Vec<RowKey>) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let Some(selection) = editing.selection.as_ref() else {
            return;
        };
        let Some(model) = selection.model() else {
            return;
        };
        let n = model.n_items();
        for position in 0..n {
            let Some(row) = model
                .item(position)
                .and_then(|item| item.downcast::<crate::ui::row_object::RowObject>().ok())
            else {
                continue;
            };
            let matches = match row.draft_id() {
                Some(draft_id) => keys
                    .iter()
                    .any(|key| matches!(key, RowKey::Draft(id) if *id == draft_id)),
                None => {
                    let pk_indices = self.result_pk_indices();
                    let cells = row.cells_clone();
                    let key = pk_indices
                        .iter()
                        .map(|&index| cells.get(index).cloned())
                        .collect::<Option<Vec<_>>>()
                        .and_then(|values| RowKey::from_pk_values(&values));
                    key.is_some_and(|key| keys.iter().any(|changed| changed == &key))
                }
            };
            if matches {
                self.refresh_result_row(position);
            }
        }
    }

    pub(in crate::ui::editor) fn handle_commit_result_edits(&mut self, sender: ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        if editing.save_in_flight.is_some() {
            return;
        }
        let Some(table) = editing.table.clone() else {
            return;
        };
        let columns = editing.mutation_columns.clone();
        let driver_id = editing.driver_id.clone();
        let tab_id = editing.tab_id;
        let result = editing.change_trackers.with_tab_ref(tab_id, |tracker| {
            tracker.materialize(&driver_id, table.schema.as_deref(), &table.name, &columns)
        });
        match result {
            Some(Ok((statements, sources))) if !statements.is_empty() => {
                let save_id = Uuid::new_v4();
                if let Some(editing) = self.result_editing.as_mut() {
                    editing.save_in_flight = Some(save_id);
                    editing.pending.save_button.set_sensitive(false);
                    editing.pending.discard_button.set_sensitive(false);
                }
                let _ = sender.output(SqlEditorOutput::ExecuteTransaction {
                    save_id,
                    statements,
                    sources,
                });
            }
            Some(Ok(_)) => self.refresh_result_pending_bar(0, Some(&sender)),
            Some(Err(error)) => {
                let _ = sender.output(SqlEditorOutput::ShowAlert {
                    title: bookie_app_services::tr!("Cannot save"),
                    body: format!("{error}"),
                });
            }
            None => {}
        }
    }

    pub(in crate::ui::editor) fn handle_preview_result_edits(&self, sender: &ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let Some(table) = editing.table.as_ref() else {
            return;
        };
        let result = editing.change_trackers.with_tab_ref(editing.tab_id, |tracker| {
            tracker.materialize(
                &editing.driver_id,
                table.schema.as_deref(),
                &table.name,
                &editing.mutation_columns,
            )
        });
        match result {
            Some(Ok((statements, _))) if !statements.is_empty() => {
                let body = statements
                    .iter()
                    .map(|(sql, _)| sql.as_str())
                    .collect::<Vec<_>>()
                    .join(";\n");
                let _ = sender.output(SqlEditorOutput::ShowAlert {
                    title: bookie_app_services::tr!("Preview SQL"),
                    body,
                });
            }
            Some(Err(error)) => {
                let _ = sender.output(SqlEditorOutput::ShowAlert {
                    title: bookie_app_services::tr!("Cannot preview"),
                    body: format!("{error}"),
                });
            }
            _ => {}
        }
    }

    pub(in crate::ui::editor) fn handle_discard_result_edits(&mut self, sender: ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_mut() else {
            return;
        };
        editing
            .change_trackers
            .with_tab(editing.tab_id, |tracker| tracker.clear());
        editing.pending.save_button.set_sensitive(true);
        editing.pending.discard_button.set_sensitive(true);
        self.refresh_result_pending_bar(0, Some(&sender));
        self.reread_result_statement(sender);
    }

    pub(in crate::ui::editor) fn handle_result_save_completed(&mut self, save_id: Uuid, sender: ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_mut() else {
            return;
        };
        if editing.save_in_flight != Some(save_id) {
            return;
        }
        editing.save_in_flight = None;
        editing.pending.save_button.set_sensitive(true);
        editing.pending.discard_button.set_sensitive(true);
        editing
            .change_trackers
            .with_tab(editing.tab_id, |tracker| tracker.clear());
        self.refresh_result_pending_bar(0, Some(&sender));
        self.reread_result_statement(sender);
    }

    pub(in crate::ui::editor) fn handle_result_save_failed(
        &mut self,
        save_id: Uuid,
        message: String,
        sender: ComponentSender<Self>,
    ) {
        let Some(editing) = self.result_editing.as_mut() else {
            return;
        };
        if editing.save_in_flight != Some(save_id) {
            return;
        }
        editing.save_in_flight = None;
        editing.pending.save_button.set_sensitive(true);
        editing.pending.discard_button.set_sensitive(true);
        let _ = sender.output(SqlEditorOutput::ShowAlert {
            title: bookie_app_services::tr!("Save failed"),
            body: message,
        });
    }

    pub(in crate::ui::editor) fn handle_result_flash_error(&mut self, source: StatementSource) {
        let key = match source {
            StatementSource::Insert { draft_id } => RowKey::Draft(draft_id),
            StatementSource::Update { row_key } | StatementSource::Delete { row_key } => row_key,
        };
        self.on_result_changed_rows(vec![key]);
    }

    fn reread_result_statement(&mut self, sender: ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_mut() else {
            return;
        };
        if editing.sql.trim().is_empty() {
            return;
        }
        let sql = editing.sql.clone();
        editing.rereading = true;
        self.begin_run(sql, None, true, sender);
        if let Some(editing) = self.result_editing.as_mut() {
            editing.rereading = false;
        }
    }
}

#[cfg(test)]
mod save_guard_tests {
    use super::should_block_mutation_during_save;
    use crate::ui::grid::GridMsg;

    #[test]
    fn every_result_mutation_is_blocked_while_a_save_is_in_flight() {
        let messages = [
            GridMsg::CellEdited {
                row_position: 0,
                col_index: 0,
                new_value: String::new(),
                row_key: Vec::new(),
            },
            GridMsg::CellEditedValue {
                row_position: 0,
                col_index: 0,
                value: bookie_core::Value::Null,
                row_key: Vec::new(),
                draft_id: None,
            },
            GridMsg::SetCellNull {
                row_position: 0,
                col_index: 0,
                row_key: Vec::new(),
                draft_id: None,
            },
            GridMsg::DeleteRowAt {
                row_position: 0,
                row_key: Vec::new(),
                draft_id: None,
            },
            GridMsg::InsertRow,
            GridMsg::DuplicateRow {
                row_position: 0,
                row_key: Vec::new(),
                draft_id: None,
            },
        ];

        assert!(
            messages
                .iter()
                .all(|message| should_block_mutation_during_save(message, true))
        );
        assert!(
            messages
                .iter()
                .all(|message| !should_block_mutation_during_save(message, false))
        );
    }

    #[test]
    fn non_mutating_grid_actions_remain_available_during_a_save() {
        assert!(!should_block_mutation_during_save(
            &GridMsg::SortChanged(Vec::new()),
            true
        ));
    }
}
