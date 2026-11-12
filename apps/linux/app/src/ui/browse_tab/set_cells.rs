use relm4::ComponentSender;
use relm4::gtk::{self, glib};

use bookie_app_services::grid_replace::{ReplaceEdit, ReplaceReport, validate_replace_plan};
use bookie_app_services::set_cells::{
    MAX_SET_CELLS, SetCellTarget, SetCellsError, SetCellsPlan, SetCellsRequest as PlanRequest, SetCellsValue,
    plan_set_cells,
};

use crate::services::change_tracker::RowKey;
use crate::ui::grid::{SetCellsRequest, SetCellsRows};
use crate::ui::row_store::RowStore;

use super::*;

const CHUNK_ROWS: usize = 256;

#[derive(Debug)]
pub struct SetCellsPlanned {
    pub(super) result: Result<SetCellsPlan, SetCellsError>,
    pub(super) generation: u64,
    pub(super) page: Option<uuid::Uuid>,
}

struct PlanJob {
    sender: ComponentSender<BrowseTab>,
    columns: Vec<ColumnInfo>,
    driver_id: String,
    table_editable: bool,
    value: SetCellsValue,
    generation: u64,
    page: Option<uuid::Uuid>,
}

struct Collector {
    store: RowStore,
    positions: Vec<u32>,
    next: usize,
    columns: Vec<usize>,
    pk: Vec<usize>,
    targets: Vec<SetCellTarget>,
    job: Option<PlanJob>,
}

fn row_key(store: &RowStore, pk: &[usize], position: u32) -> Option<Vec<Value>> {
    if matches!(store.row_key_at(position, pk), Some(RowKey::Draft(_))) {
        return None;
    }
    pk.iter().map(|column| store.cell_at(position, *column)).collect()
}

impl Collector {
    fn step(&mut self) -> glib::ControlFlow {
        let Some(job) = self.job.as_ref() else {
            return glib::ControlFlow::Break;
        };
        if self.store.generation() != job.generation {
            let _ = job.sender.output(BrowseTabOutput::ShowToast(rows_changed_text()));
            return glib::ControlFlow::Break;
        }
        let end = (self.next + CHUNK_ROWS).min(self.positions.len());
        for position in self.positions[self.next..end].iter().copied() {
            let key = row_key(&self.store, &self.pk, position);
            for column in &self.columns {
                self.targets.push(SetCellTarget {
                    position,
                    column: *column,
                    row_key: key.clone(),
                    original: self
                        .store
                        .cell_at(position, *column)
                        .unwrap_or_else(|| Value::Undecodable("not fetched".into())),
                });
            }
        }
        self.next = end;
        if end < self.positions.len() {
            return glib::ControlFlow::Continue;
        }
        if let Some(job) = self.job.take() {
            spawn_plan(job, std::mem::take(&mut self.targets));
        }
        glib::ControlFlow::Break
    }
}

fn spawn_plan(job: PlanJob, targets: Vec<SetCellTarget>) {
    glib::spawn_future_local(async move {
        let PlanJob {
            sender,
            columns,
            driver_id,
            table_editable,
            value,
            generation,
            page,
        } = job;
        let planned = gtk::gio::spawn_blocking(move || {
            let hooks = hooks_for(&driver_id);
            plan_set_cells(PlanRequest {
                columns: &columns,
                table_editable,
                value: &value,
                targets: &targets,
                hooks: &hooks,
                cell_allows_edit: crate::ui::grid::cell_allows_inline_edit,
            })
        })
        .await;
        match planned {
            Ok(result) => sender.input(BrowseTabInput::SetCellsPlanned(SetCellsPlanned {
                result,
                generation,
                page,
            })),
            Err(_) => {
                let _ = sender.output(BrowseTabOutput::ShowToast(bookie_app_services::tr!(
                    "Setting the cells could not be completed"
                )));
            }
        }
    });
}

fn rows_changed_text() -> String {
    bookie_app_services::tr!("The rows changed, so no cells were set. Try again.")
}

pub(super) fn error_text(error: &SetCellsError) -> String {
    match error {
        SetCellsError::TableNotEditable => {
            bookie_app_services::tr!("This table cannot be edited, so no cells were set.")
        }
        SetCellsError::NothingSelected => bookie_app_services::tr!("No cells are selected."),
        SetCellsError::TooManyCells => bookie_app_services::tr!("That is too many cells to set at once"),
        SetCellsError::ReadOnlyColumn { column } => bookie_app_services::tr!(
            "{column} is a key, generated or identity column and cannot be set. No cells were changed."
        )
        .replace("{column}", column),
        SetCellsError::NotNullable { column } => {
            bookie_app_services::tr!("{column} does not accept NULL. No cells were changed.")
                .replace("{column}", column)
        }
        SetCellsError::NotEditable { column, position } => {
            bookie_app_services::tr!("{column} on row {row} can't be edited, so no cells were changed.")
                .replace("{column}", column)
                .replace("{row}", &(position + 1).to_string())
        }
        SetCellsError::InvalidValue { column, error } => {
            bookie_app_services::tr!("{column}: {message}. No cells were changed.")
                .replace("{column}", column)
                .replace("{message}", &input_error_message(error.clone()))
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

impl BrowseTab {
    pub(super) fn handle_grid_set_cells(&mut self, request: SetCellsRequest, sender: ComponentSender<Self>) {
        let rows = match &request.rows {
            SetCellsRows::AllLoaded => self.row_count_loaded() as usize,
            SetCellsRows::Positions(positions) => positions.len(),
        };
        let cells = rows.saturating_mul(request.columns.len());
        let body = bookie_app_services::trn!(
            "{n} cell will be set. Nothing is saved until you click Save.",
            "{n} cells will be set. Nothing is saved until you click Save.",
            cells
        )
        .replace("{n}", &cells.to_string());
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
                sender.input(BrowseTabInput::SetCellsConfirmed {
                    request: request.clone(),
                    value: dialog_value(&entry, &null_check),
                });
            }
        });
        dialog.present(Some(&self.inner_stack));
    }

    pub(super) fn handle_set_cells_confirmed(
        &mut self,
        request: SetCellsRequest,
        value: SetCellsValue,
        sender: ComponentSender<Self>,
    ) {
        let Some(store) = self.row_store() else { return };
        let positions: Vec<u32> = match request.rows {
            SetCellsRows::AllLoaded => (0..store.n_items()).collect(),
            SetCellsRows::Positions(positions) => positions,
        };
        if positions.len().saturating_mul(request.columns.len()) > MAX_SET_CELLS {
            let _ = sender.output(BrowseTabOutput::ShowToast(error_text(&SetCellsError::TooManyCells)));
            return;
        }
        let job = PlanJob {
            sender,
            columns: self.current_columns.clone(),
            driver_id: self.driver_id.clone(),
            table_editable: self.allows_keyed_row_mutations(),
            value,
            generation: store.generation(),
            page: self.page_requests.current_id(),
        };
        let mut collector = Collector {
            store,
            positions,
            next: 0,
            columns: request.columns,
            pk: self.pk_indices(),
            targets: Vec::new(),
            job: Some(job),
        };
        glib::idle_add_local(move || collector.step());
    }

    pub(super) fn handle_set_cells_planned(&mut self, planned: SetCellsPlanned, sender: ComponentSender<Self>) {
        let toast = |message: String| {
            let _ = sender.output(BrowseTabOutput::ShowToast(message));
        };
        let current = self.row_store().map(|store| store.generation());
        if current != Some(planned.generation) || planned.page != self.page_requests.current_id() {
            toast(rows_changed_text());
            return;
        }
        let plan = match planned.result {
            Ok(plan) => plan,
            Err(error) => {
                toast(error_text(&error));
                return;
            }
        };
        if plan.edits.is_empty() {
            toast(bookie_app_services::tr!("Every selected cell already has that value."));
            return;
        }
        if !self.stage_set_cells(&plan) {
            toast(rows_changed_text());
            return;
        }
        let count = plan.edits.len();
        let mut message = bookie_app_services::trn!(
            "Set {n} cell. Review and Save to apply.",
            "Set {n} cells. Review and Save to apply.",
            count
        )
        .replace("{n}", &count.to_string());
        if plan.skipped > 0 {
            message.push(' ');
            message.push_str(
                &bookie_app_services::tr!("{n} skipped (new rows or rows without a readable key).")
                    .replace("{n}", &plan.skipped.to_string()),
            );
        }
        toast(message);
    }

    fn stage_set_cells(&mut self, plan: &SetCellsPlan) -> bool {
        let Some(store) = self.row_store() else {
            return false;
        };
        let pk = self.pk_indices();
        let mut report = ReplaceReport::default();
        report.edits = plan
            .edits
            .iter()
            .map(|edit| ReplaceEdit {
                position: edit.position,
                column: edit.column,
                original: edit.original.clone(),
                new_value: String::new(),
                matches: 1,
                row_key: edit.row_key.clone(),
            })
            .collect();
        let valid = validate_replace_plan(&report, |position, edits| {
            let key = pk
                .iter()
                .map(|column| store.cell_at(position, *column))
                .collect::<Option<Vec<_>>>()?;
            let cells = edits
                .iter()
                .map(|edit| store.cell_at(position, edit.column))
                .collect::<Option<Vec<_>>>()?;
            Some((key, cells))
        });
        if valid.is_err() {
            return false;
        }
        let edits: Option<Vec<_>> = plan
            .edits
            .iter()
            .map(|edit| {
                let key = RowKey::from_pk_values(&edit.row_key)?;
                Some((key, edit.column, edit.original.clone(), edit.value.clone()))
            })
            .collect();
        let Some(edits) = edits else { return false };
        self.change_trackers
            .with_tab(self.tab_id, |tracker| tracker.track_cell_edits(edits))
            .is_some()
    }
}
