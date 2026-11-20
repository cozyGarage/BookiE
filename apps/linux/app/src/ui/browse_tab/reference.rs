use relm4::ComponentSender;

use bookie_core::Value;

use super::*;

pub(crate) use bookie_app_services::foreign_key_nav::{ReferencedRowTarget, reference_targets, referencing_targets};

impl BrowseTab {
    pub(super) fn go_to_reference(&self, column: &str, row: &[(String, Value)], sender: &ComponentSender<Self>) {
        let output = match reference_targets(&self.foreign_keys, self.schema.as_deref(), column, row) {
            Ok(mut targets) if targets.len() == 1 => {
                let target = targets.remove(0);
                BrowseTabOutput::OpenReferencedRow {
                    schema: target.schema,
                    table: target.table,
                    filter: target.filter,
                }
            }
            Ok(targets) => BrowseTabOutput::ChooseReferencedRow(targets),
            Err(message) => BrowseTabOutput::ShowToast(message),
        };
        let _ = sender.output(output);
    }

    pub(super) fn show_referencing_rows(&self, row: &[(String, Value)], sender: &ComponentSender<Self>) {
        let output = match referencing_targets(&self.inbound_foreign_keys, row) {
            Ok(mut targets) if targets.len() == 1 => {
                let target = targets.remove(0);
                BrowseTabOutput::OpenReferencedRow {
                    schema: target.schema,
                    table: target.table,
                    filter: target.filter,
                }
            }
            Ok(targets) => BrowseTabOutput::ChooseReferencedRow(targets),
            Err(message) => BrowseTabOutput::ShowToast(message),
        };
        let _ = sender.output(output);
    }
}
