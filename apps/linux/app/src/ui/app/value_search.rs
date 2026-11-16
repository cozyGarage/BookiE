use relm4::ComponentSender;

use super::{App, AppMsg};

impl App {
    pub(super) fn on_search_database_values(&self, sender: &ComponentSender<Self>) {
        if !self.connected {
            self.show_toast(&bookie_app_services::tr!("Connect to a database first."));
            return;
        }
        let open_sender = sender.input_sender().clone();
        crate::ui::value_search::present(
            &self.window,
            crate::ui::value_search::SearchDialogRequest {
                connection_id: self.connection_id,
                capabilities: crate::ui::structure_tab::driver_capabilities_for(&self.registry, self.driver_id()),
                driver_id: self.driver_id().to_string(),
                scope: self.catalog_dialog_cancel.borrow().clone(),
                hidden_schemas: self.hidden_schema_names(),
            },
            &self.database,
            &self.preferences,
            move |schema, table, filter| {
                let _ = open_sender.send(AppMsg::OpenReferencedRow { schema, table, filter });
            },
        );
    }
}
