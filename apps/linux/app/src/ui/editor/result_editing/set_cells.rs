use bookie_app_services::set_cells::{
    MAX_SET_CELLS, SetCellTarget, SetCellsError, SetCellsPlan, SetCellsRequest as PlanRequest, SetCellsValue,
    plan_set_cells,
};
use bookie_core::Value;
use relm4::ComponentSender;
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

use super::super::{SqlEditor, SqlEditorInput, SqlEditorOutput};
use crate::services::change_tracker::RowKey;
use crate::ui::grid::{GridMsg, SetCellsRequest, SetCellsRows};

impl SqlEditor {
    pub(super) fn dispatch_set_cells(&mut self, message: GridMsg, sender: ComponentSender<Self>) {
        match message {
            GridMsg::SetCells(request) => self.request_set_cells(request, sender),
            GridMsg::SetCellsConfirmed {
                request,
                value,
                generation,
                run,
            } => self.confirm_set_cells(request, value, generation, run, sender),
            _ => {}
        }
    }
}

fn dialog_value(entry: &gtk::Entry, null_check: &gtk::CheckButton) -> SetCellsValue {
    if null_check.is_active() {
        SetCellsValue::Null
    } else {
        SetCellsValue::Text(entry.text().to_string())
    }
}

fn present_set_cells_dialog(
    request: SetCellsRequest,
    count: usize,
    generation: u64,
    run: u64,
    parent: &gtk::Box,
    sender: ComponentSender<SqlEditor>,
) {
    let body = bookie_app_services::trn!(
        "One cell will be set. Nothing is saved until you click Save.",
        "{n} cells will be set. Nothing is saved until you click Save.",
        count
    )
    .replace("{n}", &count.to_string());
    let dialog = adw::AlertDialog::new(Some(&bookie_app_services::tr!("Set Cells to Value")), Some(&body));
    let entry = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("Value"))
        .activates_default(true)
        .build();
    let null_check = gtk::CheckButton::with_label(&bookie_app_services::tr!("Set to NULL"));
    let entry_for_toggle = entry.clone();
    null_check.connect_toggled(move |check| entry_for_toggle.set_sensitive(!check.is_active()));
    let extra = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    extra.append(&entry);
    extra.append(&null_check);
    dialog.set_extra_child(Some(&extra));
    dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
    dialog.add_response("apply", &bookie_app_services::tr!("Set Cells"));
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("apply"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_, response| {
        if response == "apply" {
            sender.input(SqlEditorInput::Grid(GridMsg::SetCellsConfirmed {
                request: request.clone(),
                value: dialog_value(&entry, &null_check),
                generation,
                run,
            }));
        }
    });
    dialog.present(Some(parent));
}

impl SqlEditor {
    pub(super) fn request_set_cells(&self, request: SetCellsRequest, sender: ComponentSender<Self>) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let Some(selection) = editing.selection.as_ref() else {
            return;
        };
        let Some(store) = selection
            .model()
            .and_then(|model| model.downcast::<crate::ui::row_store::RowStore>().ok())
        else {
            return;
        };
        let rows = match &request.rows {
            SetCellsRows::AllLoaded => selection.n_items() as usize,
            SetCellsRows::Positions(positions) => positions.len(),
        };
        let count = rows.saturating_mul(request.columns.len());
        if count == 0 {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "Select cells first."
            )));
            return;
        }
        if count > MAX_SET_CELLS {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "That is too many cells."
            )));
            return;
        }
        let generation = store.generation();
        let run = editing.run;
        present_set_cells_dialog(request, count, generation, run, self.results_surface(), sender);
    }

    pub(super) fn confirm_set_cells(
        &self,
        request: SetCellsRequest,
        value: SetCellsValue,
        generation: u64,
        run: u64,
        sender: ComponentSender<Self>,
    ) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        if editing.save_in_flight.is_some() {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "Saving changes. Edit again when the save finishes."
            )));
            return;
        }
        let Some(selection) = editing.selection.as_ref() else {
            return;
        };
        let Some(model) = selection.model() else { return };
        let Some(store) = model.clone().downcast::<crate::ui::row_store::RowStore>().ok() else {
            return;
        };
        if !result_is_current(run, generation, editing.run, store.generation()) {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "The result changed. Select cells again."
            )));
            return;
        }
        let positions = match request.rows {
            SetCellsRows::AllLoaded => (0..model.n_items()).collect::<Vec<_>>(),
            SetCellsRows::Positions(positions) => positions,
        };
        let count = positions.len().saturating_mul(request.columns.len());
        if count == 0 {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "Select cells first."
            )));
            return;
        }
        if count > MAX_SET_CELLS {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "That is too many cells."
            )));
            return;
        }
        let pk = editing
            .columns
            .iter()
            .enumerate()
            .filter_map(|(index, column)| column.primary_key.then_some(index))
            .collect::<Vec<_>>();
        let targets = collect_targets(&model, &positions, &request.columns, &pk);
        let columns = editing.columns.clone();
        let driver_id = editing.driver_id.clone();
        let sender = sender.clone();
        spawn_plan(columns, driver_id, value, targets, generation, run, sender);
    }

    pub(in crate::ui::editor) fn finish_set_cells(
        &mut self,
        result: Result<SetCellsPlan, SetCellsError>,
        generation: u64,
        run: u64,
        sender: &ComponentSender<Self>,
    ) {
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let current_generation = editing
            .selection
            .as_ref()
            .and_then(|selection| selection.model())
            .and_then(|model| model.downcast::<crate::ui::row_store::RowStore>().ok())
            .map(|store| store.generation());
        if editing.run != run || current_generation != Some(generation) {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "The result changed. Select cells again."
            )));
            return;
        }
        let plan = match result {
            Ok(plan) => plan,
            Err(error) => {
                let _ = sender.output(SqlEditorOutput::ShowToast(set_cells_error(&error)));
                return;
            }
        };
        if plan.edits.is_empty() {
            let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                "Every selected cell already has that value."
            )));
            return;
        }
        let changes = plan
            .edits
            .iter()
            .filter_map(|edit| {
                RowKey::from_pk_values(&edit.row_key)
                    .map(|key| (key, edit.column, edit.original.clone(), edit.value.clone()))
            })
            .collect::<Vec<_>>();
        let Some(editing) = self.result_editing.as_ref() else {
            return;
        };
        let tab_id = editing.tab_id;
        if editing
            .change_trackers
            .with_tab(tab_id, |tracker| tracker.track_cell_edits(changes))
            .is_none()
        {
            return;
        }
        let text = bookie_app_services::trn!(
            "Set {n} cell. Review and Save to apply.",
            "Set {n} cells. Review and Save to apply.",
            plan.edits.len()
        )
        .replace("{n}", &plan.edits.len().to_string());
        let _ = sender.output(SqlEditorOutput::ShowToast(text));
    }
}

fn result_is_current(expected_run: u64, expected_generation: u64, run: u64, generation: u64) -> bool {
    expected_run == run && expected_generation == generation
}

fn spawn_plan(
    columns: Vec<bookie_core::ColumnInfo>,
    driver_id: String,
    value: SetCellsValue,
    targets: Vec<SetCellTarget>,
    generation: u64,
    run: u64,
    sender: ComponentSender<SqlEditor>,
) {
    gtk::glib::spawn_future_local(async move {
        let planned = gtk::gio::spawn_blocking(move || {
            let hooks = crate::ui::browse_tab::hooks_for(&driver_id);
            plan_set_cells(PlanRequest {
                columns: &columns,
                table_editable: true,
                value: &value,
                targets: &targets,
                hooks: &hooks,
                cell_allows_edit: crate::ui::grid::cell_allows_set_cells,
            })
        })
        .await;
        match planned {
            Ok(result) => sender.input(SqlEditorInput::SetCellsPlanned {
                result,
                generation,
                run,
            }),
            Err(_) => {
                let _ = sender.output(SqlEditorOutput::ShowToast(bookie_app_services::tr!(
                    "Setting the cells could not be completed"
                )));
            }
        }
    });
}

fn collect_targets(
    model: &gtk::gio::ListModel,
    positions: &[u32],
    columns: &[usize],
    pk: &[usize],
) -> Vec<SetCellTarget> {
    let mut targets = Vec::with_capacity(positions.len().saturating_mul(columns.len()));
    for position in positions {
        let row = model
            .item(*position)
            .and_then(|item| item.downcast::<crate::ui::row_object::RowObject>().ok());
        let row_key = row.as_ref().and_then(|row| {
            pk.iter()
                .map(|index| row.with_cell_value(*index, Clone::clone))
                .collect::<Option<Vec<Value>>>()
        });
        for column in columns {
            let original = row.as_ref().and_then(|row| row.with_cell_value(*column, Clone::clone));
            targets.push(SetCellTarget {
                position: *position,
                column: *column,
                row_key: original.as_ref().and(row_key.clone()),
                original: original.unwrap_or_else(|| Value::Undecodable("not fetched".into())),
            });
        }
    }
    targets
}

fn set_cells_error(error: &SetCellsError) -> String {
    match error {
        SetCellsError::TableNotEditable => bookie_app_services::tr!("This result cannot be edited."),
        SetCellsError::NothingSelected => bookie_app_services::tr!("Select cells first."),
        SetCellsError::TooManyCells => bookie_app_services::tr!("That is too many cells."),
        SetCellsError::ReadOnlyColumn { column } => {
            bookie_app_services::tr!("{column} is read-only.").replace("{column}", column)
        }
        SetCellsError::NotNullable { column } => {
            bookie_app_services::tr!("{column} cannot be NULL.").replace("{column}", column)
        }
        SetCellsError::NotEditable { column, position } => {
            bookie_app_services::tr!("{column} at row {row} cannot be edited.")
                .replace("{column}", column)
                .replace("{row}", &(position + 1).to_string())
        }
        SetCellsError::InvalidValue { column, .. } => {
            bookie_app_services::tr!("The value is invalid for {column}.").replace("{column}", column)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::result_is_current;

    #[test]
    fn confirmation_is_rejected_after_the_result_changes() {
        assert!(!result_is_current(4, 8, 5, 8));
        assert!(!result_is_current(4, 8, 4, 9));
        assert!(result_is_current(4, 8, 4, 8));
    }
}
