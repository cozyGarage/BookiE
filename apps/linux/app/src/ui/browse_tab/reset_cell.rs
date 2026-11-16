use bookie_core::sql_dialect::{ColumnReset, build_keyed_column_reset};
use relm4::{ComponentSender, adw, gtk};

use super::*;
use crate::services::change_tracker::{RowKey, StatementSource};
use crate::ui::grid::ResetCellRequest;

const RUN_RESPONSE: &str = "run";
const REVIEW_RESPONSE: &str = "review";
const CANCEL_RESPONSE: &str = "cancel";

impl BrowseTab {
    pub(super) fn handle_grid_reset_cell(&mut self, target: ResetCellRequest, sender: ComponentSender<Self>) {
        if self.reject_stale_row_edit(target.row_position, &target.row_key, &sender) {
            return;
        }
        if self
            .change_trackers
            .with_tab_ref(self.tab_id, |tracker| tracker.has_pending())
            .unwrap_or(false)
        {
            self.toast(
                &sender,
                bookie_app_services::tr!("Save or discard your pending changes first."),
            );
            return;
        }
        match target.reset.clone() {
            Some(reset) => self.review_reset(&target, &reset, &sender),
            None => self.prompt_expression(target, &sender),
        }
    }

    pub(super) fn handle_grid_reset_confirmed(
        &mut self,
        row_key: RowKey,
        statement: (String, Vec<Value>),
        sender: ComponentSender<Self>,
    ) {
        if self.save_in_flight.get().is_some() {
            return;
        }
        let save_id = uuid::Uuid::new_v4();
        self.save_in_flight.set(Some(save_id));
        self.save_button.set_sensitive(false);
        self.discard_button.set_sensitive(false);
        self.capture_focus_for_restore();
        let _ = sender.output(BrowseTabOutput::ExecuteTransaction {
            save_id,
            statements: vec![statement],
            sources: vec![StatementSource::Update { row_key }],
        });
    }

    fn toast(&self, sender: &ComponentSender<Self>, message: String) {
        let _ = sender.output(BrowseTabOutput::ShowToast(message));
    }

    fn review_reset(&self, target: &ResetCellRequest, reset: &ColumnReset, sender: &ComponentSender<Self>) {
        let Some((row_key, _)) = self.row_key_at(target.row_position) else {
            self.toast(sender, bookie_app_services::tr!("This row has no primary key."));
            return;
        };
        let statement = match build_keyed_column_reset(
            &self.driver_id,
            self.schema.as_deref(),
            &self.table,
            &self.current_columns,
            target.col_index,
            reset,
            &target.row_key,
        ) {
            Ok(statement) => statement,
            Err(error) => {
                self.toast(sender, crate::services::error_text::build_sql_message(&error));
                return;
            }
        };
        let dialog = adw::AlertDialog::builder()
            .heading(bookie_app_services::tr!("Run this statement?"))
            .body(preview::preview_text(&self.driver_id, std::slice::from_ref(&statement)))
            .build();
        dialog.add_response(CANCEL_RESPONSE, &bookie_app_services::tr!("Cancel"));
        dialog.add_response(RUN_RESPONSE, &bookie_app_services::tr!("Run"));
        dialog.set_response_appearance(RUN_RESPONSE, adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some(CANCEL_RESPONSE));
        dialog.set_close_response(CANCEL_RESPONSE);
        let sender = sender.clone();
        dialog.connect_response(None, move |_, response| {
            if response == RUN_RESPONSE {
                sender.input(BrowseTabInput::GridResetConfirmed {
                    row_key: row_key.clone(),
                    statement: statement.clone(),
                });
            }
        });
        dialog.present(Some(&self.pending_revealer));
    }

    fn prompt_expression(&self, target: ResetCellRequest, sender: &ComponentSender<Self>) {
        let entry = gtk::Entry::builder()
            .placeholder_text(bookie_app_services::tr!("SQL expression, for example now()"))
            .activates_default(true)
            .build();
        let dialog = adw::AlertDialog::builder()
            .heading(bookie_app_services::tr!("Set cell to an SQL expression"))
            .body(bookie_app_services::tr!(
                "The expression runs as SQL. You review the statement before it runs."
            ))
            .extra_child(&entry)
            .build();
        dialog.add_response(CANCEL_RESPONSE, &bookie_app_services::tr!("Cancel"));
        dialog.add_response(REVIEW_RESPONSE, &bookie_app_services::tr!("Review"));
        dialog.set_response_appearance(REVIEW_RESPONSE, adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some(REVIEW_RESPONSE));
        dialog.set_close_response(CANCEL_RESPONSE);
        let sender = sender.clone();
        dialog.connect_response(None, move |_, response| {
            if response == REVIEW_RESPONSE {
                sender.input(BrowseTabInput::GridResetCell(ResetCellRequest {
                    reset: Some(ColumnReset::Expression(entry.text().to_string())),
                    ..target.clone()
                }));
            }
        });
        dialog.present(Some(&self.pending_revealer));
    }
}
