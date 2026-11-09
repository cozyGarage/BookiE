use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};

use crate::services::catalog::CatalogOrigin;
use crate::ui::app::msg::CatalogDdlMsg;
use crate::ui::app::{App, AppMsg};

fn connect_confirmation_response(dialog: &adw::AlertDialog, on_response: impl Fn(&str) + 'static) {
    dialog.connect_response(None, move |_, response| on_response(response));
}

fn write_refusal(app: &App) -> Option<String> {
    if !app.connected {
        return Some(crate::tr!("Connect to a database first."));
    }
    if app.read_only {
        return Some(crate::tr!("This connection is open read-only."));
    }
    None
}

fn drop_schema_confirmation_body(driver_id: &str) -> String {
    if driver_id == "mssql" {
        return crate::tr!("The schema must be empty before it can be dropped.");
    }
    crate::tr!("All objects in this schema will be removed. This can't be undone from inside BookiE.")
}

fn run_catalog_sql(
    app: &App,
    origin: CatalogOrigin,
    sql: String,
    failure_title: String,
    on_ok: AppMsg,
    sender: ComponentSender<App>,
) {
    let connection = origin.connection(&app.database);
    let timeout_secs =
        crate::services::operation_control::timeout_for(&app.preferences, &app.database, app.connection_id);
    let result_sender = sender.clone();
    sender.command(move |_, shutdown| {
        shutdown
            .register(async move {
                let Some(conn) = connection else {
                    result_sender.input(AppMsg::ShowAlert {
                        title: failure_title,
                        body: crate::tr!("No active connection."),
                    });
                    return;
                };
                let control = crate::services::operation_control::bounded(timeout_secs);
                match conn.execute_controlled(&sql, &control).await {
                    Ok(_) => result_sender.input(on_ok),
                    Err(error) => result_sender.input(AppMsg::ShowAlert {
                        title: failure_title,
                        body: format!("{error}"),
                    }),
                }
            })
            .drop_on_shutdown()
    });
}

impl App {
    pub(super) fn dispatch_catalog_ddl(&mut self, msg: CatalogDdlMsg, sender: ComponentSender<Self>) {
        match msg {
            CatalogDdlMsg::NewSchemaPrompt => self.on_new_schema_prompt(sender),
            CatalogDdlMsg::CreateSchemaConfirmed { origin, name } => {
                self.on_create_schema_confirmed(origin, name, sender)
            }
            CatalogDdlMsg::DropSchemaPrompt { schema } => self.on_drop_schema_prompt(schema, sender),
            CatalogDdlMsg::DropSchemaConfirmed { origin, schema } => {
                self.on_drop_schema_confirmed(origin, schema, sender)
            }
            CatalogDdlMsg::NewDatabasePrompt => self.on_new_database_prompt(sender),
            CatalogDdlMsg::CreateDatabaseConfirmed { origin, name } => {
                self.on_create_database_confirmed(origin, name, sender)
            }
            CatalogDdlMsg::DropDatabasePrompt { database } => self.on_drop_database_prompt(database, sender),
            CatalogDdlMsg::DropDatabaseConfirmed { origin, database } => {
                self.on_drop_database_confirmed(origin, database, sender)
            }
        }
    }

    pub(super) fn on_new_schema_prompt(&mut self, sender: ComponentSender<Self>) {
        if let Some(message) = write_refusal(self) {
            self.show_error_alert(&crate::tr!("Cannot create schema"), &message);
            return;
        }
        let Some(origin) = CatalogOrigin::capture(self.connection_id, &self.database) else {
            return;
        };
        let driver_id = self.driver_id().to_string();
        if let Err(error) = bookie_core::sql_ddl::build_create_schema(&driver_id, "new_schema") {
            self.show_error_alert(&crate::tr!("Cannot create schema"), &format!("{error}"));
            return;
        }
        let dialog = adw::AlertDialog::new(Some(&crate::tr!("New Schema")), None);
        let entry = gtk::Entry::builder().activates_default(true).build();
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", &crate::tr!("Cancel"));
        dialog.add_response("create", &crate::tr!("Create"));
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        dialog.set_close_response("cancel");
        connect_confirmation_response(&dialog, move |response| {
            let name = entry.text().trim().to_string();
            if response == "create" && !name.is_empty() {
                sender.input(AppMsg::CatalogDdl(CatalogDdlMsg::CreateSchemaConfirmed {
                    origin: origin.clone(),
                    name,
                }));
            }
        });
        dialog.present(Some(&self.window));
    }

    pub(super) fn on_create_schema_confirmed(
        &mut self,
        origin: CatalogOrigin,
        name: String,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        if let Some(message) = write_refusal(self) {
            self.show_error_alert(&crate::tr!("Cannot create schema"), &message);
            return;
        }
        let Some(driver_id) = self.current_driver_id.clone() else {
            return;
        };
        let sql = match bookie_core::sql_ddl::build_create_schema(&driver_id, &name) {
            Ok(sql) => sql,
            Err(error) => {
                self.show_error_alert(&crate::tr!("Cannot create schema"), &format!("{error}"));
                return;
            }
        };
        run_catalog_sql(
            self,
            origin.clone(),
            sql,
            crate::tr!("Create schema failed"),
            AppMsg::SchemaChanged {
                origin,
                schema: Some(name),
                table: None,
            },
            sender,
        );
    }

    pub(super) fn on_drop_schema_prompt(&mut self, schema: String, sender: ComponentSender<Self>) {
        if let Some(message) = write_refusal(self) {
            self.show_error_alert(&crate::tr!("Cannot drop schema"), &message);
            return;
        }
        let Some(origin) = CatalogOrigin::capture(self.connection_id, &self.database) else {
            return;
        };
        let driver_id = self.driver_id();
        if bookie_core::sql_ddl::protects_schema(driver_id, &schema) {
            self.show_error_alert(
                &crate::tr!("Cannot drop schema"),
                &crate::tr!("\"{schema}\" is a system schema and cannot be dropped.").replace("{schema}", &schema),
            );
            return;
        }
        if let Err(error) = bookie_core::sql_ddl::build_drop_schema(driver_id, &schema, true) {
            self.show_error_alert(&crate::tr!("Cannot drop schema"), &format!("{error}"));
            return;
        }
        let title = crate::tr!("Drop {schema}?").replace("{schema}", &schema);
        let body = drop_schema_confirmation_body(driver_id);
        let dialog = adw::AlertDialog::new(Some(&title), Some(&body));
        dialog.add_response("cancel", &crate::tr!("Cancel"));
        dialog.add_response("drop", &crate::tr!("Drop"));
        dialog.set_response_appearance("drop", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        connect_confirmation_response(&dialog, move |response| {
            if response == "drop" {
                sender.input(AppMsg::CatalogDdl(CatalogDdlMsg::DropSchemaConfirmed {
                    origin: origin.clone(),
                    schema: schema.clone(),
                }));
            }
        });
        dialog.present(Some(&self.window));
    }

    pub(super) fn on_drop_schema_confirmed(
        &mut self,
        origin: CatalogOrigin,
        schema: String,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        if let Some(message) = write_refusal(self) {
            self.show_error_alert(&crate::tr!("Cannot drop schema"), &message);
            return;
        }
        let Some(driver_id) = self.current_driver_id.clone() else {
            return;
        };
        let sql = match bookie_core::sql_ddl::build_drop_schema(&driver_id, &schema, true) {
            Ok(sql) => sql,
            Err(error) => {
                self.show_error_alert(&crate::tr!("Cannot drop schema"), &format!("{error}"));
                return;
            }
        };
        run_catalog_sql(
            self,
            origin.clone(),
            sql,
            crate::tr!("Drop schema failed"),
            AppMsg::SchemaChanged {
                origin,
                schema: None,
                table: None,
            },
            sender,
        );
    }

    pub(super) fn on_new_database_prompt(&mut self, sender: ComponentSender<Self>) {
        if let Some(message) = write_refusal(self) {
            self.show_error_alert(&crate::tr!("Cannot create database"), &message);
            return;
        }
        let Some(origin) = CatalogOrigin::capture(self.connection_id, &self.database) else {
            return;
        };
        let driver_id = self.driver_id().to_string();
        if let Err(error) = bookie_core::sql_ddl::build_create_database(&driver_id, "new_database") {
            self.show_error_alert(&crate::tr!("Cannot create database"), &format!("{error}"));
            return;
        }
        let dialog = adw::AlertDialog::new(Some(&crate::tr!("New Database")), None);
        let entry = gtk::Entry::builder().activates_default(true).build();
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", &crate::tr!("Cancel"));
        dialog.add_response("create", &crate::tr!("Create"));
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        dialog.set_close_response("cancel");
        connect_confirmation_response(&dialog, move |response| {
            let name = entry.text().trim().to_string();
            if response == "create" && !name.is_empty() {
                sender.input(AppMsg::CatalogDdl(CatalogDdlMsg::CreateDatabaseConfirmed {
                    origin: origin.clone(),
                    name,
                }));
            }
        });
        dialog.present(Some(&self.window));
    }

    pub(super) fn on_create_database_confirmed(
        &mut self,
        origin: CatalogOrigin,
        name: String,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        if let Some(message) = write_refusal(self) {
            self.show_error_alert(&crate::tr!("Cannot create database"), &message);
            return;
        }
        let Some(driver_id) = self.current_driver_id.clone() else {
            return;
        };
        let sql = match bookie_core::sql_ddl::build_create_database(&driver_id, &name) {
            Ok(sql) => sql,
            Err(error) => {
                self.show_error_alert(&crate::tr!("Cannot create database"), &format!("{error}"));
                return;
            }
        };
        run_catalog_sql(
            self,
            origin,
            sql,
            crate::tr!("Create database failed"),
            AppMsg::LoadDatabases,
            sender,
        );
    }

    pub(super) fn on_drop_database_prompt(&mut self, database: String, sender: ComponentSender<Self>) {
        if let Some(message) = write_refusal(self) {
            self.show_error_alert(&crate::tr!("Cannot drop database"), &message);
            return;
        }
        let Some(origin) = CatalogOrigin::capture(self.connection_id, &self.database) else {
            return;
        };
        let driver_id = self.driver_id();
        let current = self.active_database_name();
        if database == current {
            self.show_error_alert(
                &crate::tr!("Cannot drop database"),
                &crate::tr!("Switch to another database before dropping the one this window is using."),
            );
            return;
        }
        if bookie_core::sql_ddl::protects_database(driver_id, &database) {
            self.show_error_alert(
                &crate::tr!("Cannot drop database"),
                &crate::tr!("\"{database}\" is a system database and cannot be dropped.")
                    .replace("{database}", &database),
            );
            return;
        }
        if let Err(error) = bookie_core::sql_ddl::build_drop_database(driver_id, &database) {
            self.show_error_alert(&crate::tr!("Cannot drop database"), &format!("{error}"));
            return;
        }
        let title = crate::tr!("Drop {database}?").replace("{database}", &database);
        let body = crate::tr!(
            "This database and all of its objects will be removed. This can't be undone from inside BookiE."
        );
        let dialog = adw::AlertDialog::new(Some(&title), Some(&body));
        dialog.add_response("cancel", &crate::tr!("Cancel"));
        dialog.add_response("drop", &crate::tr!("Drop"));
        dialog.set_response_appearance("drop", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        connect_confirmation_response(&dialog, move |response| {
            if response == "drop" {
                sender.input(AppMsg::CatalogDdl(CatalogDdlMsg::DropDatabaseConfirmed {
                    origin: origin.clone(),
                    database: database.clone(),
                }));
            }
        });
        dialog.present(Some(&self.window));
    }

    pub(super) fn on_drop_database_confirmed(
        &mut self,
        origin: CatalogOrigin,
        database: String,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        if let Some(message) = write_refusal(self) {
            self.show_error_alert(&crate::tr!("Cannot drop database"), &message);
            return;
        }
        let Some(driver_id) = self.current_driver_id.clone() else {
            return;
        };
        let sql = match bookie_core::sql_ddl::build_drop_database(&driver_id, &database) {
            Ok(sql) => sql,
            Err(error) => {
                self.show_error_alert(&crate::tr!("Cannot drop database"), &format!("{error}"));
                return;
            }
        };
        run_catalog_sql(
            self,
            origin,
            sql,
            crate::tr!("Drop database failed"),
            AppMsg::LoadDatabases,
            sender,
        );
    }
}

pub(crate) fn droppable_database(name: &str, current: &str, driver_id: &str) -> bool {
    name != current
        && bookie_core::sql_ddl::supports_drop_database(driver_id)
        && !bookie_core::sql_ddl::protects_database(driver_id, name)
}

#[cfg(test)]
mod tests {
    use super::{drop_schema_confirmation_body, droppable_database};

    #[test]
    fn schema_drop_confirmation_matches_engine_behavior() {
        assert_eq!(
            drop_schema_confirmation_body("mssql"),
            "The schema must be empty before it can be dropped."
        );
        assert_eq!(
            drop_schema_confirmation_body("duckdb"),
            "All objects in this schema will be removed. This can't be undone from inside BookiE."
        );
    }

    #[test]
    fn the_current_and_system_databases_are_not_offered_for_drop() {
        assert!(!droppable_database("app", "app", "postgres"));
        assert!(!droppable_database("template1", "app", "postgres"));
        assert!(droppable_database("analytics", "app", "postgres"));
        assert!(!droppable_database("analytics", "app", "sqlite"));
    }
}
