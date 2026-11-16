use std::rc::Rc;

use relm4::ComponentSender;
use relm4::adw::prelude::*;

use super::{App, AppMsg};
use crate::ui::health_report_dialog::{HealthReportSource, present};

const ACTION: &str = "win.show-health-report";

impl App {
    fn health_report_supported(&self) -> bool {
        self.current_driver_id
            .as_deref()
            .and_then(|id| self.registry.get(id))
            .is_some_and(|driver| driver.capabilities().supports_table_health_report)
    }

    pub(super) fn sync_health_report_action(&self) {
        self.window.action_set_enabled(ACTION, self.health_report_supported());
    }

    pub(super) fn show_health_report(&self, sender: &ComponentSender<Self>) {
        let Some(connection) = self.connection_id.filter(|_| self.health_report_supported()) else {
            self.show_toast(&bookie_app_services::tr!(
                "The table and index health report is available for PostgreSQL connections."
            ));
            return;
        };
        let editor_sender = sender.clone();
        present(
            self.window.upcast_ref::<relm4::gtk::Window>(),
            HealthReportSource {
                connection,
                database: self.database.clone(),
                preferences: self.preferences.clone(),
                hidden_schemas: self.hidden_schema_names(),
                open_in_editor: Rc::new(move |sql| editor_sender.input(AppMsg::OpenSqlInEditor(sql))),
            },
        );
    }
}
