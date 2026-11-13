use relm4::adw::prelude::*;
use relm4::{ComponentController, ComponentSender, adw};
use uuid::Uuid;

use bookie_core::{ColumnInfo, Value};
use bookie_storage::query_history::{Outcome, Source};

use crate::services::read_scopes::ReadScope;

use crate::services::catalog::CatalogOrigin;
use crate::services::error_text;
use crate::ui::app::ran_statements::finish_if_recorded;
use crate::ui::app::{App, AppMsg, WorkspaceTab};
use crate::ui::structure_tab::{StructureMode, StructureTabInput};

fn connect_confirmation_response(dialog: &adw::AlertDialog, on_response: impl Fn(&str) + 'static) {
    dialog.connect_response(None, move |_, response| on_response(response));
}

#[cfg(test)]
#[path = "structure_confirmation_tests.rs"]
mod confirmation_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructureText {
    CreateTable,
    JoinSkeleton,
}

impl App {
    pub(super) fn on_show_er_diagram(&self, schema: Option<String>) {
        crate::ui::diagram::present(
            self.window.upcast_ref::<relm4::gtk::Window>(),
            crate::ui::diagram::DiagramRequest {
                schema,
                connection_id: self.connection_id,
                capabilities: crate::ui::structure_tab::driver_capabilities_for(&self.registry, self.driver_id()),
                scope: self.catalog_dialog_cancel.borrow().clone(),
                hidden_schemas: self.hidden_schema_names(),
            },
            &self.database,
            &self.preferences,
        );
    }

    pub(super) fn on_compare_schema(&self, schema: Option<String>, sender: &ComponentSender<Self>) {
        let script_sender = sender.input_sender().clone();
        crate::ui::schema_compare::present(
            self.window.upcast_ref::<relm4::gtk::Window>(),
            crate::ui::schema_compare::CompareRequest {
                schema,
                connection_id: self.connection_id,
                driver_id: self.driver_id().to_string(),
                capabilities: crate::ui::structure_tab::driver_capabilities_for(&self.registry, self.driver_id()),
                scope: self.catalog_dialog_cancel.borrow().clone(),
                hidden_schemas: self.hidden_schema_names(),
            },
            &self.database,
            &self.preferences,
            move |sql| {
                let _ = script_sender.send(AppMsg::ShowCreateTableLoaded { sql });
            },
        );
    }

    pub(super) fn on_new_table_tab(&mut self, schema: Option<String>, sender: ComponentSender<Self>) {
        if !self.connected {
            self.show_toast(&bookie_app_services::tr!("Connect to a database first."));
            return;
        }
        self.append_new_structure_tab(schema, sender);
    }

    pub(super) fn on_edit_structure_tab(
        &mut self,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        if !self.connected {
            self.show_toast(&bookie_app_services::tr!("Connect to a database first."));
            return;
        }
        let front = self.connection_id;
        let existing_structure = self.workspace_tabs.borrow().iter().find_map(|(id, tab)| match tab {
            WorkspaceTab::Structure(slot)
                if slot.mode == StructureMode::Edit
                    && slot.schema == schema
                    && slot.table == table
                    && self.tab_belongs_to(*id, front) =>
            {
                Some(slot.page.clone())
            }
            _ => None,
        });
        if let Some(page) = existing_structure
            && let Some(tab_view) = self.workspace_tab_view.as_ref()
        {
            tab_view.set_selected_page(&page);
            return;
        }
        self.append_existing_structure_tab(schema, table, sender);
    }

    fn has_open_tabs_for_table(&self, connection_id: Uuid, schema: Option<&str>, table: &str) -> bool {
        self.workspace_tabs.borrow().iter().any(|(id, tab)| {
            if !self.tab_belongs_to(*id, Some(connection_id)) {
                return false;
            }
            match tab {
                WorkspaceTab::Structure(s) => s.schema.as_deref() == schema && s.table == table,
                WorkspaceTab::Table(s) => s.schema.as_deref() == schema && s.table == table,
                _ => false,
            }
        })
    }

    pub(super) fn on_truncate_table_prompt(
        &mut self,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        let Some(origin) = CatalogOrigin::capture(self.connection_id, &self.database) else {
            return;
        };
        let driver_id = self.driver_id().to_string();
        if let Err(e) = bookie_core::sql_ddl::build_truncate_table(&driver_id, schema.as_deref(), &table) {
            self.show_error_alert(&bookie_app_services::tr!("Cannot truncate table"), &format!("{e}"));
            return;
        }
        let title = bookie_app_services::tr!("Truncate {table}?").replace("{table}", &table);
        let body = bookie_app_services::tr!(
            "All rows will be removed and the table definition kept. This can't be undone from inside BookiE."
        );
        let dialog = adw::AlertDialog::new(Some(&title), Some(&body));
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("truncate", &bookie_app_services::tr!("Truncate"));
        dialog.set_response_appearance("truncate", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        connect_confirmation_response(&dialog, move |response| {
            if response == "truncate" {
                sender.input(AppMsg::TruncateTableConfirmed {
                    origin: origin.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                });
            }
        });
        dialog.present(Some(&self.window));
    }

    pub(super) fn on_truncate_table_confirmed(
        &mut self,
        origin: CatalogOrigin,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        let Some(driver_id) = self.current_driver_id.clone() else {
            return;
        };
        let sql = match bookie_core::sql_ddl::build_truncate_table(&driver_id, schema.as_deref(), &table) {
            Ok(sql) => sql,
            Err(e) => {
                self.show_error_alert(&bookie_app_services::tr!("Cannot truncate table"), &format!("{e}"));
                return;
            }
        };
        let connection = origin.connection(&self.database);
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let sender_for_cmd = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let Some(conn) = connection else {
                        sender_for_cmd.input(AppMsg::ShowAlert {
                            title: bookie_app_services::tr!("Cannot truncate table"),
                            body: bookie_app_services::tr!("No active connection."),
                        });
                        return;
                    };
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    match conn.execute_controlled(&sql, &control).await {
                        Ok(_) => sender_for_cmd.input(AppMsg::SchemaChanged {
                            origin,
                            schema,
                            table: Some(table),
                        }),
                        Err(e) => sender_for_cmd.input(AppMsg::ShowAlert {
                            title: bookie_app_services::tr!("Truncate failed"),
                            body: format!("{e}"),
                        }),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_rename_table_prompt(
        &mut self,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        let Some(origin) = CatalogOrigin::capture(self.connection_id, &self.database) else {
            return;
        };
        if self.has_open_tabs_for_table(origin.id, schema.as_deref(), &table) {
            self.show_error_alert(
                &bookie_app_services::tr!("Cannot rename table"),
                &bookie_app_services::tr!("Close the open tabs for {table} first, so no pending changes are lost.")
                    .replace("{table}", &table),
            );
            return;
        }
        let title = bookie_app_services::tr!("Rename {table}").replace("{table}", &table);
        let dialog = adw::AlertDialog::new(Some(&title), None);
        let entry = gtk4::Entry::builder().text(&table).activates_default(true).build();
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("rename", &bookie_app_services::tr!("Rename"));
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");
        connect_confirmation_response(&dialog, move |response| {
            let new_name = entry.text().trim().to_string();
            if response == "rename" && new_name != table {
                sender.input(AppMsg::RenameTableConfirmed {
                    origin: origin.clone(),
                    schema: schema.clone(),
                    table: table.clone(),
                    new_name,
                });
            }
        });
        dialog.present(Some(&self.window));
    }

    pub(super) fn on_rename_table_confirmed(
        &mut self,
        origin: CatalogOrigin,
        schema: Option<String>,
        table: String,
        new_name: String,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        let Some(driver_id) = self.current_driver_id.clone() else {
            return;
        };
        let sql = match bookie_core::sql_ddl::build_rename_table(&driver_id, schema.as_deref(), &table, &new_name) {
            Ok(sql) => sql,
            Err(e) => {
                self.show_error_alert(&bookie_app_services::tr!("Cannot rename table"), &format!("{e}"));
                return;
            }
        };
        let connection = origin.connection(&self.database);
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let sender_for_cmd = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let Some(conn) = connection else {
                        sender_for_cmd.input(AppMsg::ShowAlert {
                            title: bookie_app_services::tr!("Cannot rename table"),
                            body: bookie_app_services::tr!("No active connection."),
                        });
                        return;
                    };
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    match conn.execute_controlled(&sql, &control).await {
                        Ok(_) => sender_for_cmd.input(AppMsg::SchemaChanged {
                            origin,
                            schema,
                            table: None,
                        }),
                        Err(e) => sender_for_cmd.input(AppMsg::ShowAlert {
                            title: bookie_app_services::tr!("Rename failed"),
                            body: format!("{e}"),
                        }),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_drop_table_prompt(
        &mut self,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        let Some(origin) = CatalogOrigin::capture(self.connection_id, &self.database) else {
            return;
        };
        let connection = origin.connection(&self.database);
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let sender_for_cmd = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    let inbound = match connection {
                        Some(conn) => conn
                            .fetch_inbound_foreign_keys_controlled(schema.as_deref(), &table, &control)
                            .await
                            .unwrap_or_default(),
                        None => Vec::new(),
                    };
                    let referencing = bookie_app_services::referencing_tables::referencing_tables(
                        &inbound,
                        schema.as_deref(),
                        &table,
                    );
                    sender_for_cmd.input(AppMsg::DropTablePromptReady {
                        origin,
                        schema,
                        table,
                        referencing,
                    });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn present_drop_table_prompt(
        &mut self,
        origin: CatalogOrigin,
        schema: Option<String>,
        table: String,
        referencing: &[String],
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        let title = bookie_app_services::tr!("Drop {table}?").replace("{table}", &table);
        let body = drop_table_body(referencing);
        let dialog = adw::AlertDialog::new(Some(&title), Some(&body));
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("drop", &bookie_app_services::tr!("Drop"));
        dialog.set_response_appearance("drop", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let sender_for_resp = sender.clone();
        let schema_for_resp = schema.clone();
        let table_for_resp = table.clone();
        connect_confirmation_response(&dialog, move |response| {
            if response == "drop" {
                sender_for_resp.input(AppMsg::DropTableConfirmed {
                    origin: origin.clone(),
                    schema: schema_for_resp.clone(),
                    table: table_for_resp.clone(),
                });
            }
        });
        dialog.present(Some(&self.window));
    }

    /// Close tabs for the table only after DROP succeeds.
    pub(super) fn on_drop_table_confirmed(
        &mut self,
        origin: CatalogOrigin,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        let Some(driver_id) = self.current_driver_id.clone() else {
            return;
        };
        let sql = match bookie_core::sql_ddl::build_drop_table(&driver_id, schema.as_deref(), &table, true, false) {
            Ok(s) => s,
            Err(e) => {
                self.show_error_alert(&bookie_app_services::tr!("Cannot drop table"), &format!("{e}"));
                return;
            }
        };
        let schema_for_msg = schema.clone();
        let table_for_msg = table.clone();
        let sender_for_cmd = sender.clone();
        let connection = origin.connection(&self.database);
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let Some(conn) = connection else {
                        sender_for_cmd.input(AppMsg::ShowAlert {
                            title: bookie_app_services::tr!("Cannot drop table"),
                            body: bookie_app_services::tr!("No active connection."),
                        });
                        return;
                    };
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    match conn.execute_controlled(&sql, &control).await {
                        Ok(_) => {
                            sender_for_cmd.input(AppMsg::DropTableSucceeded {
                                origin: origin.clone(),
                                schema: schema_for_msg.clone(),
                                table: table_for_msg.clone(),
                            });
                        }
                        Err(e) => {
                            sender_for_cmd.input(AppMsg::ShowAlert {
                                title: bookie_app_services::tr!("Drop failed"),
                                body: format!("{e}"),
                            });
                        }
                    }
                })
                .drop_on_shutdown()
        });
    }

    /// Save a background Structure tab selected for close-with-pending.
    pub(super) fn save_structure_tab_by_id(&mut self, tab_id: Uuid, sender: ComponentSender<Self>) {
        // Let the active save resolve close-after-save instead of re-dispatching.
        if self.structure_saves_in_flight.borrow().contains(&tab_id) {
            tracing::debug!(
                ?tab_id,
                "save_structure_tab_by_id: save already in flight; deferring to first dispatch"
            );
            return;
        }
        let Some(driver_id) = self.tab_driver_id(tab_id) else {
            self.report_refused_tab_save(tab_id, &sender);
            return;
        };
        let result = self
            .structure_trackers
            .with_tab_ref(tab_id, |t| t.materialize(&driver_id));
        match result {
            Some(Ok(statements)) if !statements.is_empty() => {
                self.on_execute_structure_transaction(tab_id, statements, sender);
            }
            Some(Ok(_)) => {
                // Continue close-after-save when there is nothing to commit.
                sender.input(AppMsg::StructureSaveCompleted {
                    origin: self.tab_origin(tab_id),
                    tab_id,
                    new_table_name: None,
                });
            }
            Some(Err(e)) => {
                sender.input(AppMsg::StructureSaveFailed(tab_id, format!("{e}")));
            }
            None => {}
        }
    }

    /// Close matching tabs before refreshing the sidebar after a successful drop.
    pub(super) fn on_drop_table_succeeded(
        &mut self,
        origin: CatalogOrigin,
        schema: Option<String>,
        table: String,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        self.close_tabs_for_table(origin.id, schema.as_deref(), &table);
        sender.input(AppMsg::SchemaChanged {
            origin: origin.clone(),
            schema,
            table: Some(table),
        });
    }

    /// Skip pending-change prompts because the dropped table no longer exists.
    pub(super) fn close_tabs_for_table(&mut self, connection_id: Uuid, schema: Option<&str>, table: &str) {
        let Some(tab_view) = self.workspace_tab_view.clone() else {
            return;
        };
        let mut targets: Vec<Uuid> = Vec::new();
        for (id, tab) in self.workspace_tabs.borrow().iter() {
            if !self.tab_belongs_to(*id, Some(connection_id)) {
                continue;
            }
            match tab {
                WorkspaceTab::Structure(s) if s.schema.as_deref() == schema && s.table == table => targets.push(*id),
                WorkspaceTab::Table(s) if s.schema.as_deref() == schema && s.table == table => targets.push(*id),
                _ => {}
            }
        }
        for id in targets {
            self.finish_close_workspace_tab_now(id, &tab_view);
        }
    }

    /// Run transactional DDL as one batch; run non-transactional DDL sequentially.
    pub(super) fn on_execute_structure_transaction(
        &mut self,
        tab_id: Uuid,
        statements: Vec<String>,
        sender: ComponentSender<Self>,
    ) {
        // Prevent concurrent saves from committing the same DDL twice.
        if !self.structure_saves_in_flight.borrow_mut().insert(tab_id) {
            tracing::debug!(?tab_id, "structure save already in flight; ignoring duplicate");
            return;
        }
        let Some(driver_id) = self.tab_driver_id(tab_id) else {
            self.structure_saves_in_flight.borrow_mut().remove(&tab_id);
            self.report_refused_tab_save(tab_id, &sender);
            return;
        };
        let ddl_is_transactional = self
            .registry
            .get(&driver_id)
            .is_some_and(|driver| driver.capabilities().ddl_is_transactional);
        let mode = {
            let tabs = self.workspace_tabs.borrow();
            let Some(slot) = tabs.get(&tab_id) else {
                self.structure_saves_in_flight.borrow_mut().remove(&tab_id);
                return;
            };
            match slot {
                WorkspaceTab::Structure(s) => s.mode,
                // A table-tab save is stale because DDL now uses Structure tabs.
                _ => {
                    self.structure_saves_in_flight.borrow_mut().remove(&tab_id);
                    return;
                }
            }
        };
        // The first CreateTable op carries the name for a new-table draft.
        let new_table_name = if matches!(mode, StructureMode::New) {
            self.structure_trackers
                .with_tab_ref(tab_id, |t| {
                    t.ops().iter().find_map(|op| {
                        if let bookie_core::sql_ddl::StructureOp::CreateTable { table, .. } = op {
                            Some(table.clone())
                        } else {
                            None
                        }
                    })
                })
                .flatten()
        } else {
            None
        };

        let Some(origin) = self.tab_origin(tab_id) else {
            self.structure_saves_in_flight.borrow_mut().remove(&tab_id);
            sender.input(AppMsg::StructureSaveFailed(
                tab_id,
                bookie_app_services::tr!("No active connection."),
            ));
            return;
        };
        self.in_flight_saves.set(self.in_flight_saves.get() + 1);
        let sender_for_cmd = sender.clone();
        let mut recorded = self.ran_statements(tab_id, Source::Structure, &statements);
        let connection = origin.connection(&self.database);
        let timeout_secs = self.tab_timeout_secs(tab_id);
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let Some(conn) = connection else {
                        sender_for_cmd.input(AppMsg::StructureSaveFailed(
                            tab_id,
                            bookie_app_services::tr!("No active connection."),
                        ));
                        return;
                    };
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    if ddl_is_transactional {
                        let batch: Vec<(String, Vec<Value>)> =
                            statements.iter().map(|sql| (sql.clone(), Vec::new())).collect();
                        if let Err(e) = conn.execute_in_transaction_controlled(&batch, &control).await {
                            let msg = error_text::driver_message(&e);
                            finish_if_recorded(&mut recorded, Outcome::Error(msg.clone())).await;
                            sender_for_cmd.input(AppMsg::StructureSaveFailed(tab_id, msg));
                            return;
                        }
                    } else {
                        for sql in &statements {
                            if let Err(e) = conn.execute_controlled(sql, &control).await {
                                let msg = error_text::driver_message(&e);
                                finish_if_recorded(&mut recorded, Outcome::Error(msg.clone())).await;
                                sender_for_cmd.input(AppMsg::StructureSaveFailed(tab_id, msg));
                                return;
                            }
                        }
                    }
                    finish_if_recorded(&mut recorded, Outcome::Success).await;
                    sender_for_cmd.input(AppMsg::StructureSaveCompleted {
                        origin: Some(origin.clone()),
                        tab_id,
                        new_table_name: new_table_name.clone(),
                    });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_structure_save_completed(
        &mut self,
        origin: Option<CatalogOrigin>,
        tab_id: Uuid,
        new_table_name: Option<String>,
        sender: ComponentSender<Self>,
    ) {
        if self.in_flight_saves.get() > 0 {
            self.in_flight_saves.set(self.in_flight_saves.get() - 1);
        }
        self.structure_saves_in_flight.borrow_mut().remove(&tab_id);

        let Some(origin) = origin.filter(|origin| self.tab_owns_origin(tab_id, origin)) else {
            return;
        };

        enum SaveKind {
            PromoteNewToTable(Option<String>, String),
            UpdateInPlace(Option<String>, String),
            Skip,
        }
        let kind = {
            let tabs = self.workspace_tabs.borrow();
            match tabs.get(&tab_id) {
                Some(WorkspaceTab::Structure(slot)) => match new_table_name.clone() {
                    Some(name) => SaveKind::PromoteNewToTable(slot.schema.clone(), name),
                    None => SaveKind::UpdateInPlace(slot.schema.clone(), slot.table.clone()),
                },
                Some(WorkspaceTab::Table(slot)) => SaveKind::UpdateInPlace(slot.schema.clone(), slot.table.clone()),
                _ => SaveKind::Skip,
            }
        };

        let kind_promoted = matches!(kind, SaveKind::PromoteNewToTable(..));
        match kind {
            SaveKind::PromoteNewToTable(schema, name) => {
                if let Some(tab_view) = self.workspace_tab_view.clone() {
                    self.finish_close_workspace_tab_now(tab_id, &tab_view);
                }
                super::App::append_table_tab(
                    self,
                    schema.clone(),
                    name.clone(),
                    super::types::TableTabStart::first_page(self.default_page_size, false),
                    sender.clone(),
                );
                sender.input(AppMsg::SchemaChanged {
                    schema,
                    table: Some(name),
                    origin: origin.clone(),
                });
            }
            SaveKind::UpdateInPlace(schema, table) => {
                if let Some(controller) = self
                    .workspace_tabs
                    .borrow()
                    .get(&tab_id)
                    .and_then(|t| t.structure_controller())
                {
                    let _ = controller
                        .sender()
                        .send(StructureTabInput::SaveCompleted { new_table_name });
                }
                sender.input(AppMsg::SchemaChanged {
                    schema,
                    table: Some(table),
                    origin: origin.clone(),
                });
            }
            SaveKind::Skip => {}
        }

        // Consume close-after-save state even when a new-table draft was already closed.
        let drained = self.close_after_save.borrow_mut().complete(&tab_id);
        if drained && !kind_promoted {
            self.request_workspace_tab_close(tab_id);
        }
        if self.close_window_after_save.get() && self.close_after_save.borrow().is_empty() {
            self.close_window_after_save.set(false);
            self.window.close();
        }
    }

    pub(super) fn on_structure_save_failed(&mut self, tab_id: Uuid, message: String) {
        if self.in_flight_saves.get() > 0 {
            self.in_flight_saves.set(self.in_flight_saves.get() - 1);
        }
        self.structure_saves_in_flight.borrow_mut().remove(&tab_id);
        // A failed save must not let a later tab completion close this window.
        self.close_after_save.borrow_mut().cancel(&tab_id);
        self.close_window_after_save.set(false);
        self.announce_save_failure(tab_id, &message);
        if let Some(controller) = self
            .workspace_tabs
            .borrow()
            .get(&tab_id)
            .and_then(|t| t.structure_controller())
        {
            let _ = controller.sender().send(StructureTabInput::SaveFailed(message));
        }
    }

    pub(super) fn on_structure_text(
        &self,
        schema: Option<String>,
        table: String,
        text: StructureText,
        sender: ComponentSender<Self>,
    ) {
        match text {
            StructureText::CreateTable => self.on_show_create_table(schema, table, sender),
            StructureText::JoinSkeleton => self.on_show_join_skeleton(schema, table, sender),
        }
    }

    fn on_show_join_skeleton(&self, schema: Option<String>, table: String, sender: ComponentSender<Self>) {
        let driver_id = self.driver_id().to_string();
        let connection_id = self.connection_id;
        let database = self.database.clone();
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let sender_for_cmd = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let Some(conn) = connection_id.and_then(|id| database.get(id)) else {
                        sender_for_cmd.input(AppMsg::ShowToast(bookie_app_services::tr!("No active connection.")));
                        return;
                    };
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    let keys = match conn
                        .fetch_foreign_keys_controlled(schema.as_deref(), &table, &control)
                        .await
                    {
                        Ok(keys) => keys,
                        Err(error) => {
                            sender_for_cmd.input(AppMsg::ShowAlert {
                                title: bookie_app_services::tr!("Couldn't read foreign keys"),
                                body: crate::services::error_text::driver_message(&error),
                            });
                            return;
                        }
                    };
                    match bookie_core::join_skeleton(&driver_id, schema.as_deref(), &table, &keys) {
                        Ok(sql) => sender_for_cmd.input(AppMsg::ShowCreateTableLoaded { sql }),
                        Err(refusal) => sender_for_cmd.input(AppMsg::ShowAlert {
                            title: bookie_app_services::tr!("No join was built"),
                            body: refusal.to_string(),
                        }),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_show_create_table(&self, schema: Option<String>, table: String, sender: ComponentSender<Self>) {
        let driver_id = self.driver_id().to_string();
        let sender_for_cmd = sender.clone();
        let table_for_cmd = table.clone();
        let schema_for_cmd = schema.clone();
        let connection_id = self.connection_id;
        let database = self.database.clone();
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let Some(conn) = connection_id.and_then(|id| database.get(id)) else {
                        sender_for_cmd.input(AppMsg::ShowToast(bookie_app_services::tr!("No active connection.")));
                        return;
                    };
                    let control = crate::services::operation_control::bounded(timeout_secs);
                    let columns = match conn
                        .fetch_columns_controlled(schema_for_cmd.as_deref(), &table_for_cmd, &control)
                        .await
                    {
                        Ok(c) => c,
                        Err(e) => {
                            sender_for_cmd.input(AppMsg::ShowToast(
                                bookie_app_services::tr!("Couldn't read columns: {error}")
                                    .replace("{error}", &format!("{e}")),
                            ));
                            return;
                        }
                    };
                    if columns.is_empty() {
                        sender_for_cmd.input(AppMsg::ShowToast(
                            bookie_app_services::tr!("Table {table} has no columns.")
                                .replace("{table}", &table_for_cmd),
                        ));
                        return;
                    }
                    if let Err(e) = bookie_core::check_pre_dispatch(&control) {
                        sender_for_cmd.input(AppMsg::ShowToast(
                            bookie_app_services::tr!("Couldn't read indexes: {error}")
                                .replace("{error}", &format!("{e}")),
                        ));
                        return;
                    }
                    let indexes = match conn
                        .fetch_indexes_controlled(schema_for_cmd.as_deref(), &table_for_cmd, &control)
                        .await
                    {
                        Ok(indexes) => indexes,
                        Err(e) => {
                            sender_for_cmd.input(AppMsg::ShowToast(
                                bookie_app_services::tr!("Couldn't read indexes: {error}")
                                    .replace("{error}", &format!("{e}")),
                            ));
                            return;
                        }
                    };
                    if let Err(e) = bookie_core::check_pre_dispatch(&control) {
                        sender_for_cmd.input(AppMsg::ShowToast(
                            bookie_app_services::tr!("Couldn't read foreign keys: {error}")
                                .replace("{error}", &format!("{e}")),
                        ));
                        return;
                    }
                    let fks = match conn
                        .fetch_foreign_keys_controlled(schema_for_cmd.as_deref(), &table_for_cmd, &control)
                        .await
                    {
                        Ok(fks) => fks,
                        Err(e) => {
                            sender_for_cmd.input(AppMsg::ShowToast(
                                bookie_app_services::tr!("Couldn't read foreign keys: {error}")
                                    .replace("{error}", &format!("{e}")),
                            ));
                            return;
                        }
                    };
                    // Preserve fetched metadata so generated SQL retains driver-specific types and defaults.
                    let op = bookie_core::sql_ddl::StructureOp::CreateTable {
                        schema: schema_for_cmd.clone(),
                        table: table_for_cmd.clone(),
                        columns: columns
                            .into_iter()
                            .map(bookie_core::sql_ddl::DraftColumn::from_info)
                            .collect(),
                        indexes,
                        fks,
                        default_collation: None,
                    };
                    match bookie_core::sql_ddl::materialize_ops(&[op], &driver_id) {
                        Ok(stmts) if !stmts.is_empty() => {
                            let sql = stmts.join(";\n\n") + ";";
                            sender_for_cmd.input(AppMsg::ShowCreateTableLoaded { sql });
                        }
                        Ok(_) => {
                            sender_for_cmd.input(AppMsg::ShowToast(bookie_app_services::tr!("Nothing to show.")));
                        }
                        Err(e) => {
                            sender_for_cmd.input(AppMsg::ShowToast(
                                bookie_app_services::tr!("Couldn't build SQL: {error}")
                                    .replace("{error}", &format!("{e}")),
                            ));
                        }
                    }
                })
                .drop_on_shutdown()
        });
    }

    /// Fetch all structure metadata together to avoid rebuilding the tab three times.
    pub(super) fn on_fetch_structure_data(&self, tab_id: Uuid, sender: ComponentSender<Self>) {
        let (schema, table) = {
            let tabs = self.workspace_tabs.borrow();
            let Some(slot) = tabs.get(&tab_id) else {
                return;
            };
            let Some((schema, table)) = slot.schema_table() else {
                return;
            };
            (schema.map(str::to_owned), table.to_string())
        };
        let token = self.read_scopes.borrow_mut().start(tab_id, ReadScope::Structure);
        let sender_for_cmd = sender.clone();
        let Some(tab_driver_id) = self.tab_driver_id(tab_id) else {
            sender.input(AppMsg::StructureLoadFailed {
                tab_id,
                message: bookie_app_services::tr!("No active connection."),
            });
            return;
        };
        let capabilities = crate::ui::structure_tab::driver_capabilities_for(&self.registry, &tab_driver_id);
        let connection_id = self.resolve_tab_connection(tab_id).ok();
        let database = self.database.clone();
        let timeout_secs = self.tab_timeout_secs(tab_id);
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let Some(conn) = connection_id.and_then(|id| database.get(id)) else {
                        sender_for_cmd.input(AppMsg::StructureLoadFailed {
                            tab_id,
                            message: bookie_app_services::tr!("No active connection."),
                        });
                        return;
                    };
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let fail = |message: String| {
                        if !control.cancellation_token().is_cancelled() {
                            sender_for_cmd.input(AppMsg::StructureLoadFailed { tab_id, message });
                        }
                    };
                    let columns = match conn.fetch_columns_controlled(schema.as_deref(), &table, &control).await {
                        Ok(c) => c,
                        Err(e) => {
                            fail(format!("{e}"));
                            return;
                        }
                    };
                    if let Err(e) = bookie_core::check_pre_dispatch(&control) {
                        fail(format!("{e}"));
                        return;
                    }
                    let metadata = match bookie_app_services::structure_metadata::load_structure_metadata(
                        conn.as_ref(),
                        schema.as_deref(),
                        &table,
                        capabilities,
                        &control,
                    )
                    .await
                    {
                        Ok(metadata) => metadata,
                        Err(e) => {
                            fail(format!("{e}"));
                            return;
                        }
                    };
                    if control.cancellation_token().is_cancelled() {
                        return;
                    }
                    sender_for_cmd.input(AppMsg::StructureDataLoaded {
                        tab_id,
                        columns,
                        metadata,
                    });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_structure_data_loaded(
        &self,
        tab_id: Uuid,
        columns: Vec<ColumnInfo>,
        metadata: bookie_app_services::structure_metadata::StructureMetadata,
    ) {
        if let Some(controller) = self
            .workspace_tabs
            .borrow()
            .get(&tab_id)
            .and_then(|t| t.structure_controller())
        {
            let _ = controller
                .sender()
                .send(StructureTabInput::StructureLoaded { columns, metadata });
        }
    }

    pub(super) fn on_fetch_collations(&self, tab_id: Uuid, sender: ComponentSender<Self>) {
        let token = self.read_scopes.borrow_mut().start(tab_id, ReadScope::Structure);
        let sender_for_cmd = sender.clone();
        let connection_id = self.resolve_tab_connection(tab_id).ok();
        let database = self.database.clone();
        let table_default_supported = self.tab_driver_id(tab_id).is_some_and(|driver_id| {
            crate::ui::structure_tab::driver_capabilities_for(&self.registry, &driver_id)
                .supports_table_default_collation
        });
        let timeout_secs = self.tab_timeout_secs(tab_id);
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let Some(conn) = connection_id.and_then(|id| database.get(id)) else {
                        return;
                    };
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let Ok(collations) =
                        bookie_app_services::structure_metadata::load_collations(conn.as_ref(), &control).await
                    else {
                        return;
                    };
                    if control.cancellation_token().is_cancelled() {
                        return;
                    }
                    sender_for_cmd.input(AppMsg::CollationsLoaded {
                        tab_id,
                        collations: collations.into_iter().map(|collation| collation.name).collect(),
                        table_default_supported,
                    });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_collations_loaded(&self, tab_id: Uuid, collations: Vec<String>, table_default_supported: bool) {
        if let Some(controller) = self
            .workspace_tabs
            .borrow()
            .get(&tab_id)
            .and_then(|t| t.structure_controller())
        {
            let _ = controller.sender().send(StructureTabInput::CollationsLoaded {
                collations,
                table_default_supported,
            });
        }
    }

    pub(super) fn on_structure_load_failed(&self, tab_id: Uuid, message: String) {
        if let Some(controller) = self
            .workspace_tabs
            .borrow()
            .get(&tab_id)
            .and_then(|t| t.structure_controller())
        {
            let _ = controller.sender().send(StructureTabInput::LoadFailed(message));
        }
    }

    /// Reject stale schema and catalog work after a connection or request change.
    pub(super) fn on_schema_changed(
        &self,
        origin: CatalogOrigin,
        schema: Option<String>,
        table: Option<String>,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        let Some(conn) = origin.connection(&self.database) else {
            return;
        };
        self.schema_index.borrow_mut().invalidate_columns();
        let affected: Vec<Uuid> = self
            .workspace_tabs
            .borrow()
            .iter()
            .filter_map(|(id, tab)| match tab {
                WorkspaceTab::Table(slot)
                    if table
                        .as_ref()
                        .is_none_or(|table| &slot.table == table && slot.schema == schema) =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        for id in affected {
            sender.input(AppMsg::FetchBrowseColumns(id));
        }
        let generation = self.catalog_generation.get().wrapping_add(1);
        self.catalog_generation.set(generation);
        let token = crate::services::request_generation::replace_in_flight(&self.catalog_cancel);
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let hidden_schemas = self.hidden_schema_names();
        let sender_for_cmd = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let result = async {
                        let tables =
                            crate::services::catalog::list_tables_excluding(conn.as_ref(), &hidden_schemas, &control)
                                .await?;
                        let views =
                            crate::services::catalog::list_views_excluding(conn.as_ref(), &hidden_schemas, &control)
                                .await?;
                        Ok::<_, bookie_core::DriverError>((tables, views))
                    }
                    .await
                    .map_err(|error| error_text::driver_message(&error));
                    sender_for_cmd.input(AppMsg::CatalogReloaded {
                        origin,
                        generation,
                        result,
                    });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_catalog_reloaded(
        &mut self,
        origin: CatalogOrigin,
        generation: u64,
        result: Result<crate::services::catalog::Catalog, String>,
        sender: relm4::ComponentSender<App>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) || self.catalog_generation.get() != generation {
            return;
        }
        match result {
            Ok((tables, views)) => {
                self.set_completion_tables(&tables, &views);
                self.sidebar_views = views;
                self.repopulate_sidebar(&tables, sender);
                self.refresh_schema_index();
            }
            Err(error) => self.show_error_alert(&bookie_app_services::tr!("Catalog refresh failed"), &error),
        }
    }

    pub(super) fn refresh_structure_tab_dirty(&self, tab_id: Uuid, dirty: bool) {
        let schemas_count = self.sidebar_schemas_distinct();
        let tabs = self.workspace_tabs.borrow();
        let Some(slot) = tabs.get(&tab_id) else {
            return;
        };
        let (page, schema, table_name, combined_dirty) = match slot {
            WorkspaceTab::Structure(s) => (&s.page, s.schema.as_deref(), s.table.clone(), dirty),
            WorkspaceTab::Table(s) => {
                // Either data or structure edits mark the shared tab dirty.
                let data = self
                    .change_trackers
                    .with_tab_ref(s.id, |tr| tr.has_pending())
                    .unwrap_or(false);
                (&s.page, s.schema.as_deref(), s.table.clone(), dirty || data)
            }
            _ => return,
        };
        let base = if table_name.is_empty() {
            bookie_app_services::tr!("New Table")
        } else if matches!(slot, WorkspaceTab::Structure(_)) {
            super::workspace_tabs::structure_tab_title(schemas_count, schema, &table_name)
        } else {
            super::workspace_tabs::qualified_browse_tab_label(schemas_count, schema, &table_name)
        };
        let title = if combined_dirty { format!("• {base}") } else { base };
        super::tab_title::set_tab_title(page, &title);
        let is_selected = self
            .workspace_tab_view
            .as_ref()
            .and_then(|tv| tv.selected_page())
            .map(|p| &p == page)
            .unwrap_or(false);
        page.set_needs_attention(combined_dirty && !is_selected);
        self.refresh_window_title();
    }
}

fn drop_table_body(referencing: &[String]) -> String {
    let warning = bookie_app_services::tr!(
        "All rows and the table definition will be removed. This can't be undone from inside BookiE."
    );
    if referencing.is_empty() {
        return warning;
    }
    let referenced = bookie_app_services::tr!(
        "These tables reference it: {tables}. The database may refuse the drop or change their rows."
    )
    .replace("{tables}", &referencing.join(", "));
    format!("{warning}\n\n{referenced}")
}

#[cfg(test)]
mod drop_prompt_tests {
    use super::drop_table_body;

    #[test]
    fn the_drop_confirmation_names_the_tables_that_reference_the_table() {
        let plain = drop_table_body(&[]);
        let warned = drop_table_body(&["orders".into(), "billing.invoices".into()]);
        assert!(warned.starts_with(&plain));
        assert!(warned.contains("orders, billing.invoices"));
        assert!(!plain.contains("reference"));
    }
}
