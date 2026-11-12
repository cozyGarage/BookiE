use relm4::adw::prelude::*;
use relm4::{ComponentController, ComponentSender, adw};

use bookie_core::{ColumnInfo, QueryResult};
use uuid::Uuid;

use crate::services::browse_query::{BrowseTarget, PageQuery};
use crate::ui::browse_tab::{BrowseLoadFailure, BrowsePageRequest, BrowseRowCountRequest, BrowseTabInput};

use super::{App, AppMsg, ExportFormat, OpenMode};
use crate::services::read_scopes::ReadScope;

impl App {
    fn browse_cell_value_context(&self, tab_id: Uuid) -> Option<(Option<String>, String, Vec<ColumnInfo>, String)> {
        let tabs = self.workspace_tabs.borrow();
        let controller = tabs.get(&tab_id)?.browse_controller()?;
        let model = controller.model();
        Some((
            model.schema().map(str::to_owned),
            model.table().to_owned(),
            model.columns().to_vec(),
            model.driver_id().to_owned(),
        ))
    }

    pub(super) fn fetch_browse_cell_value(
        &self,
        tab_id: Uuid,
        request: crate::ui::browse_tab::BrowseCellValueRequest,
        col_index: usize,
        column_name: String,
        row_key: Vec<bookie_core::Value>,
        sender: ComponentSender<Self>,
    ) {
        let Some((schema, table, columns, driver_id)) = self.browse_cell_value_context(tab_id) else {
            return;
        };
        let query = match browse_cell_value_query(
            schema.as_deref(),
            &table,
            &columns,
            &driver_id,
            col_index,
            &column_name,
            &row_key,
        ) {
            Ok(query) => query,
            Err(message) => {
                self.fail_browse_cell_value(&sender, tab_id, request, col_index, column_name, message);
                return;
            }
        };
        let Some(conn) = self.tab_connection(tab_id) else {
            self.fail_browse_cell_value(
                &sender,
                tab_id,
                request,
                col_index,
                column_name,
                bookie_app_services::tr!("No active connection.").to_string(),
            );
            return;
        };
        let timeout_secs = self.tab_timeout_secs(tab_id);
        let token = self.read_scopes.borrow_mut().start(tab_id, ReadScope::CellValue);
        let reply = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let result = conn
                        .query_params_controlled(&query.sql, &query.params, &control)
                        .await
                        .map_err(|error| crate::services::error_text::driver_message(&error))
                        .and_then(single_cell_value);
                    reply.input(AppMsg::BrowseCellValueLoaded(
                        tab_id,
                        request,
                        col_index,
                        column_name,
                        result,
                    ));
                })
                .drop_on_shutdown()
        });
    }

    fn fail_browse_cell_value(
        &self,
        sender: &ComponentSender<Self>,
        tab_id: Uuid,
        request: crate::ui::browse_tab::BrowseCellValueRequest,
        col_index: usize,
        column_name: String,
        message: String,
    ) {
        sender.input(AppMsg::BrowseCellValueLoaded(
            tab_id,
            request,
            col_index,
            column_name,
            Err(message),
        ));
    }

    /// Sidebar click — routes via OpenMode (smart switch / new tab).
    pub(super) fn on_select_table(
        &mut self,
        schema: Option<String>,
        name: String,
        open_mode: OpenMode,
        sender: ComponentSender<Self>,
    ) {
        if self.split_view.is_collapsed() {
            self.split_view.set_show_sidebar(false);
        }
        self.dispatch_select_table(schema, name, open_mode, sender);
    }

    /// Fire the SELECT * query for a specific browse tab. Result goes to
    /// the same tab via `AppMsg::RowsLoaded(tab_id, ...)`. Composes the
    /// SELECT from the tab's current sort + filter + pagination state.
    /// Filter and sort are server-side; the row window is rendered by
    /// `sql_dialect::build_order_and_pagination` because the syntax is
    /// dialect-specific. Past `KEYSET_OFFSET_THRESHOLD`, sequential Next
    /// uses a primary-key seek when PKs and a cursor are available.
    pub(super) fn fetch_browse_page(&self, tab_id: Uuid, sender: ComponentSender<Self>) {
        let (schema, table, request, limit, sort, filter, columns, driver_id, keyset_cursor, reverse_page, last_page) = {
            let tabs = self.workspace_tabs.borrow();
            let Some(controller) = tabs.get(&tab_id).and_then(|t| t.browse_controller()) else {
                return;
            };
            let model = controller.model();
            (
                model.schema().map(str::to_owned),
                model.table().to_string(),
                model.begin_page_request(),
                model.page_size(),
                model.current_sort(),
                model.current_filter().clone(),
                model.columns().to_vec(),
                model.driver_id().to_string(),
                if model.reverse_page() {
                    model.keyset_before_cursor().map(|v| v.to_vec())
                } else {
                    model.keyset_cursor().map(|v| v.to_vec())
                },
                model.reverse_page(),
                model.last_page(),
            )
        };

        let offset = request.offset;
        let Some(conn) = self.tab_connection(tab_id) else {
            sender.input(AppMsg::LoadFailed(
                Some(tab_id),
                BrowseLoadFailure {
                    request: Some(request),
                    message: "no active connection".into(),
                },
            ));
            return;
        };
        let pending = self
            .change_trackers
            .with_tab_ref(tab_id, |tracker| tracker.has_pending())
            .unwrap_or(false);
        let hidden_columns = if pending {
            std::collections::HashSet::new()
        } else {
            self.resolve_tab_connection(tab_id)
                .ok()
                .zip(self.persistence.column_widths.as_ref())
                .map(|(connection_id, store)| store.hidden_columns(connection_id, &table))
                .unwrap_or_default()
        };
        let target = BrowseTarget {
            driver_id: &driver_id,
            schema: schema.as_deref(),
            table: &table,
            columns: &columns,
            filter: &filter,
            hidden_columns: Some(&hidden_columns),
            long_text_preview: columns.iter().any(|column| column.primary_key),
        };
        let row_id_order = self.relation_is_ordinary_table(schema.as_deref(), &table);
        let page = if reverse_page {
            target.page_before(offset, limit, sort, keyset_cursor.as_deref())
        } else if last_page {
            target.page_last(offset, limit, sort)
        } else if row_id_order {
            target.page_by_row_id(offset, limit, sort, keyset_cursor.as_deref())
        } else {
            target.page(offset, limit, sort, keyset_cursor.as_deref())
        };
        let query = match page {
            Ok(query) => query,
            Err(message) => {
                sender.input(AppMsg::LoadFailed(
                    Some(tab_id),
                    BrowseLoadFailure {
                        request: Some(request),
                        message,
                    },
                ));
                return;
            }
        };

        let timeout_secs = self.tab_timeout_secs(tab_id);
        let token = self.read_scopes.borrow_mut().start(tab_id, ReadScope::Page);
        let sender_clone = sender.clone();
        let reverse_rows = matches!(&query, PageQuery::Sql(bound) if bound.reverse_rows);
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let (result, projected_columns) = match query {
                        PageQuery::Native => (
                            conn.fetch_rows_controlled(schema.as_deref(), &table, offset, limit, &control)
                                .await,
                            None,
                        ),
                        PageQuery::Sql(query) => {
                            let projected_columns = query.projected_columns.clone();
                            let mut page = conn.query_params_controlled(&query.sql, &query.params, &control).await;
                            if let Ok(rows) = page.as_mut() {
                                crate::services::browse_query::mark_partial_cells(rows, &query.preview_columns);
                            }
                            (page, projected_columns)
                        }
                    };
                    match result {
                        Ok(mut query_result) => {
                            if reverse_rows {
                                query_result.rows.reverse();
                            }
                            sender_clone.input(AppMsg::RowsLoaded(tab_id, request, query_result, projected_columns))
                        }
                        Err(_) if control.cancellation_token().is_cancelled() => {}
                        Err(e) => sender_clone.input(AppMsg::LoadFailed(
                            Some(tab_id),
                            BrowseLoadFailure {
                                request: Some(request),
                                message: crate::services::error_text::driver_message(&e),
                            },
                        )),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn fetch_browse_columns(&self, tab_id: Uuid, sender: ComponentSender<Self>) {
        let (schema, table) = {
            let tabs = self.workspace_tabs.borrow();
            let Some(controller) = tabs.get(&tab_id).and_then(|t| t.browse_controller()) else {
                return;
            };
            let model = controller.model();
            (model.schema().map(str::to_owned), model.table().to_string())
        };

        let Some(conn) = self.tab_connection(tab_id) else {
            return;
        };
        let timeout_secs = self.tab_timeout_secs(tab_id);
        let check_view = self
            .tab_driver_id(tab_id)
            .and_then(|driver_id| self.registry.get(&driver_id))
            .is_some_and(|driver| driver.capabilities().supports_updatable_views)
            && !self.relation_is_ordinary_table(schema.as_deref(), &table);
        let token = self.read_scopes.borrow_mut().start(tab_id, ReadScope::Columns);
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    match conn.fetch_columns_controlled(schema.as_deref(), &table, &control).await {
                        Ok(_) | Err(_) if control.cancellation_token().is_cancelled() => {}
                        Ok(columns) => {
                            let plan = bookie_app_services::view_edit::resolve_view_columns(
                                &*conn,
                                schema.as_deref(),
                                &table,
                                columns,
                                check_view,
                                &control,
                            )
                            .await;
                            if control.cancellation_token().is_cancelled() {
                                return;
                            }
                            sender_clone.input(AppMsg::BrowseViewRestriction(tab_id, plan.restriction));
                            sender_clone.input(AppMsg::ColumnsLoaded(
                                tab_id,
                                plan.columns,
                                control.cancellation_token().clone(),
                            ));
                        }
                        Err(error) => sender_clone.input(AppMsg::LoadFailed(
                            Some(tab_id),
                            BrowseLoadFailure {
                                request: None,
                                message: crate::services::error_text::driver_message(&error),
                            },
                        )),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn fetch_browse_foreign_keys(&self, tab_id: Uuid, sender: ComponentSender<Self>) {
        let (schema, table) = {
            let tabs = self.workspace_tabs.borrow();
            let Some(controller) = tabs.get(&tab_id).and_then(|t| t.browse_controller()) else {
                return;
            };
            let model = controller.model();
            (model.schema().map(str::to_owned), model.table().to_string())
        };

        let Some(conn) = self.tab_connection(tab_id) else {
            return;
        };
        let timeout_secs = self.tab_timeout_secs(tab_id);
        let token = self.read_scopes.borrow_mut().start(tab_id, ReadScope::ForeignKeys);
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    // Foreign keys are used only to offer a value picker on
                    // the referencing cell -- a failed or unsupported read
                    // just means no picker, not a load failure for the tab.
                    if let Ok(foreign_keys) = conn
                        .fetch_foreign_keys_controlled(schema.as_deref(), &table, &control)
                        .await
                        && !control.cancellation_token().is_cancelled()
                    {
                        sender_clone.input(AppMsg::ForeignKeysLoaded(tab_id, foreign_keys));
                    }
                    if let Ok(inbound) = conn
                        .fetch_inbound_foreign_keys_controlled(schema.as_deref(), &table, &control)
                        .await
                        && !control.cancellation_token().is_cancelled()
                    {
                        sender_clone.input(AppMsg::InboundForeignKeysLoaded(tab_id, inbound));
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn fetch_browse_row_count(&self, tab_id: Uuid, sender: ComponentSender<Self>) {
        let (schema, table, request, filter, columns, driver_id) = {
            let tabs = self.workspace_tabs.borrow();
            let Some(controller) = tabs.get(&tab_id).and_then(|t| t.browse_controller()) else {
                return;
            };
            let model = controller.model();
            (
                model.schema().map(str::to_owned),
                model.table().to_string(),
                model.begin_row_count_request(),
                model.current_filter().clone(),
                model.columns().to_vec(),
                model.driver_id().to_string(),
            )
        };

        let Some(conn) = self.tab_connection(tab_id) else {
            return;
        };

        let target = BrowseTarget {
            driver_id: &driver_id,
            schema: schema.as_deref(),
            table: &table,
            columns: &columns,
            filter: &filter,
            hidden_columns: None,
            long_text_preview: false,
        };
        let query = match target.count() {
            Ok(query) => query,
            Err(error) => {
                sender.input(AppMsg::RowCountFailed(tab_id, request));
                sender.input(AppMsg::ShowToast(error));
                return;
            }
        };

        let timeout_secs = self.tab_timeout_secs(tab_id);
        let token = self.read_scopes.borrow_mut().start(tab_id, ReadScope::Count);
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let qr_result = run_browse_count(conn, query, &control).await;
                    let count = qr_result.ok().and_then(|qr| row_count_from_result(&qr));
                    match count {
                        None if control.cancellation_token().is_cancelled() => {}
                        Some(count) => sender_clone.input(AppMsg::RowCountLoaded(tab_id, request, count)),
                        // A stale total left on screen after a failed
                        // recount can enable Last Page past the real
                        // end of the table -- clear it instead of
                        // leaving the last successful count displayed.
                        None => sender_clone.input(AppMsg::RowCountFailed(tab_id, request)),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_browse_columns_loaded(
        &self,
        tab_id: Uuid,
        columns: Vec<ColumnInfo>,
        request: tokio_util::sync::CancellationToken,
        sender: ComponentSender<Self>,
    ) {
        if !columns_read_is_current(&request) {
            return;
        }
        let targets = crate::ui::browse_tab::scalar_enum_columns(&columns);
        self.dispatch_to_tab(tab_id, BrowseTabInput::ColumnsLoaded(columns));
        sender.input(AppMsg::FetchBrowseForeignKeys(tab_id));
        if !targets.is_empty() {
            sender.input(AppMsg::FetchBrowseEnumLabels(tab_id, targets));
        }
    }

    pub(super) fn fetch_browse_enum_labels(
        &self,
        tab_id: Uuid,
        targets: Vec<(String, bookie_core::QualifiedTypeName)>,
        sender: ComponentSender<Self>,
    ) {
        let Some(conn) = self.tab_connection(tab_id) else {
            return;
        };
        let timeout_secs = self.tab_timeout_secs(tab_id);
        let token = self.read_scopes.borrow_mut().start(tab_id, ReadScope::EnumLabels);
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, token);
                    let mut labels = Vec::new();
                    for (column, type_name) in targets {
                        if let Ok(members) = conn
                            .fetch_enum_labels_controlled(&type_name.schema, &type_name.name, &control)
                            .await
                        {
                            labels.push((column, members));
                        }
                    }
                    if !control.cancellation_token().is_cancelled() {
                        sender_clone.input(AppMsg::EnumLabelsLoaded(tab_id, labels));
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_browse_foreign_keys_loaded(&self, tab_id: Uuid, foreign_keys: Vec<bookie_core::ForeignKeyInfo>) {
        self.dispatch_to_tab(tab_id, BrowseTabInput::ForeignKeysLoaded(foreign_keys));
    }

    pub(super) fn on_browse_inbound_foreign_keys_loaded(
        &self,
        tab_id: Uuid,
        inbound: Vec<bookie_core::InboundForeignKeyInfo>,
    ) {
        self.dispatch_to_tab(tab_id, BrowseTabInput::InboundForeignKeysLoaded(inbound));
    }

    pub(super) fn on_browse_rows_loaded(
        &self,
        tab_id: Uuid,
        request: BrowsePageRequest,
        result: QueryResult,
        projected_columns: Option<Vec<usize>>,
    ) {
        self.dispatch_to_tab(
            tab_id,
            BrowseTabInput::RowsLoaded {
                request,
                result,
                projected_columns,
            },
        );
    }

    pub(super) fn on_browse_row_count_loaded(&self, tab_id: Uuid, request: BrowseRowCountRequest, count: u64) {
        self.dispatch_to_tab(tab_id, BrowseTabInput::RowCountLoaded { request, count });
    }

    pub(super) fn on_browse_row_count_failed(&self, tab_id: Uuid, request: BrowseRowCountRequest) {
        self.dispatch_to_tab(tab_id, BrowseTabInput::RowCountFailed(request));
    }

    pub(super) fn on_browse_load_failed(&mut self, tab_id: Option<Uuid>, failure: BrowseLoadFailure) {
        match tab_id {
            Some(id) => {
                if let Some(request) = failure.request {
                    let accepts = self
                        .workspace_tabs
                        .borrow()
                        .get(&id)
                        .and_then(|tab| tab.browse_controller())
                        .is_some_and(|controller| controller.model().accepts_page_request(request));
                    if !accepts {
                        return;
                    }
                }
                self.dispatch_to_tab(id, BrowseTabInput::ShowError(failure.message));
            }
            None => {
                tracing::warn!(error = %failure.message, "app-level load failed");
                self.dismiss_loading_page();
                self.set_status_page(
                    super::StatusKind::Error,
                    &bookie_app_services::tr!("Failed"),
                    &failure.message,
                );
            }
        }
    }

    pub(super) fn on_export(&self, format: ExportFormat) {
        let Some(request) = self.browse_export_request() else {
            return;
        };
        crate::ui::export_dialog::present_with_format(
            &self.window,
            &self.toast_overlay,
            request,
            matches!(format, ExportFormat::Json),
            &self.preferences,
        );
    }

    pub(super) fn on_export_front_results(&self) {
        let Some(id) = self.selected_workspace_tab_id() else {
            self.show_toast(&bookie_app_services::tr!("Nothing to export"));
            return;
        };
        let export = {
            let tabs = self.workspace_tabs.borrow();
            match tabs.get(&id) {
                Some(super::WorkspaceTab::Table(_)) => FrontExport::Browse,
                Some(super::WorkspaceTab::Editor(slot)) => match slot.controller.model().export_shown_result() {
                    Some((result, name)) => FrontExport::Editor { result, name },
                    None => FrontExport::Empty,
                },
                Some(super::WorkspaceTab::Structure(_)) | None => FrontExport::Empty,
            }
        };
        match export {
            FrontExport::Browse => {
                let Some(request) = self.browse_export_request() else {
                    return;
                };
                crate::ui::export_dialog::present(&self.window, &self.toast_overlay, request, &self.preferences);
            }
            FrontExport::Editor { result, name } => {
                let Some(driver_id) = self.tab_driver_id(id) else {
                    self.show_toast(&bookie_app_services::tr!("Nothing to export"));
                    return;
                };
                crate::ui::export_dialog::present(
                    &self.window,
                    &self.toast_overlay,
                    crate::ui::export_dialog::ExportRequest {
                        scope: crate::ui::export_dialog::ExportScope::QueryResult,
                        paged: None,
                        result,
                        suggested_name: name,
                        driver_id,
                    },
                    &self.preferences,
                )
            }
            FrontExport::Empty => self.show_toast(&bookie_app_services::tr!("Nothing to export")),
        }
    }

    fn browse_export_request(&self) -> Option<crate::ui::export_dialog::ExportRequest> {
        let Some((schema, table)) = self.selected_browse_slot_table() else {
            self.show_toast(&bookie_app_services::tr!("Nothing to export"));
            return None;
        };
        let Some(active_id) = self.selected_browse_tab_id() else {
            self.show_toast(&bookie_app_services::tr!("Nothing to export"));
            return None;
        };
        let Some(driver_id) = self.tab_driver_id(active_id) else {
            self.show_toast(&bookie_app_services::tr!("Nothing to export"));
            return None;
        };
        let (result, has_unfetched_columns, filtered) = {
            let tabs = self.workspace_tabs.borrow();
            let Some(model) = tabs
                .get(&active_id)
                .and_then(|tab| tab.browse_controller())
                .map(|controller| controller.model())
            else {
                self.show_toast(&bookie_app_services::tr!("Nothing to export"));
                return None;
            };
            (
                model.snapshot(),
                model.has_unfetched_columns(),
                !model.current_filter().is_empty(),
            )
        };
        let Some(result) = result else {
            let message = if has_unfetched_columns {
                bookie_app_services::tr!("Show hidden columns and reload before exporting this page.")
            } else {
                bookie_app_services::tr!("Nothing to export")
            };
            self.show_toast(&message);
            return None;
        };
        let table_label = match &schema {
            Some(schema) => format!("{schema}.{table}"),
            None => table,
        };
        Some(crate::ui::export_dialog::ExportRequest {
            scope: crate::ui::export_dialog::ExportScope::TablePage { filtered },
            paged: self.snapshot_export(active_id),
            result: std::sync::Arc::new(result),
            suggested_name: table_label,
            driver_id,
        })
    }

    pub(super) fn on_export_streamed_results(&self, request: crate::ui::export_dialog::StreamExportRequest) {
        let Some(spool) = request.spool else {
            self.show_toast(&bookie_app_services::tr!(
                "The complete query result is no longer available."
            ));
            return;
        };
        let fetcher =
            crate::ui::export_dialog::spool_export_factory(spool, request.columns.clone(), request.total_rows);
        let result = QueryResult {
            columns: request.columns,
            rows: Vec::new().into(),
            truncated: false,
        };
        crate::ui::export_dialog::present_with_format(
            &self.window,
            &self.toast_overlay,
            crate::ui::export_dialog::ExportRequest {
                scope: crate::ui::export_dialog::ExportScope::QueryStored,
                paged: Some(crate::ui::export_dialog::PagedExport {
                    mode: crate::ui::export_dialog::PagedExportMode::StoredQuery,
                    total_rows: Some(request.total_rows),
                    open: fetcher,
                }),
                result: std::sync::Arc::new(result),
                suggested_name: request.query.suggested_name,
                driver_id: request.query.driver_id,
            },
            false,
            &self.preferences,
        );
    }

    fn relation_is_ordinary_table(&self, schema: Option<&str>, table: &str) -> bool {
        use crate::services::sidebar_tree::NodeKind;
        self.sidebar_nodes.borrow().iter().any(|node| {
            matches!(&node.kind, NodeKind::Table { schema: node_schema, name }
                if name == table && node_schema.as_deref() == schema)
        })
    }

    fn export_page_plan(&self, tab_id: Uuid) -> Option<(crate::services::export_pages::PagePlan, Option<usize>)> {
        let tabs = self.workspace_tabs.borrow();
        let model = tabs.get(&tab_id)?.browse_controller()?.model();
        let plan = crate::services::export_pages::PagePlan {
            driver_id: model.driver_id().to_string(),
            schema: model.schema().map(str::to_owned),
            table: model.table().to_string(),
            columns: model.columns().to_vec(),
            filter: model.current_filter().clone(),
            sort: model.current_sort(),
            row_id_order: self.relation_is_ordinary_table(model.schema(), model.table()),
            timeout_secs: self.tab_timeout_secs(tab_id),
        };
        let total = model.total_rows().and_then(|total| usize::try_from(total).ok());
        Some((plan, total))
    }

    fn snapshot_export(&self, tab_id: Uuid) -> Option<crate::ui::export_dialog::PagedExport> {
        let driver_id = self.tab_driver_id(tab_id)?;
        let supported = self
            .registry
            .get(&driver_id)
            .is_some_and(|driver| driver.capabilities().supports_snapshot_export);
        if !supported {
            return None;
        }
        let guard = self
            .resolve_tab_connection(tab_id)
            .ok()
            .and_then(|id| self.database.guard(id, bookie_policy::Principal::human_gui()))?;
        let (plan, total_rows) = self.export_page_plan(tab_id)?;
        Some(crate::ui::export_dialog::PagedExport {
            mode: crate::ui::export_dialog::PagedExportMode::SnapshotTable,
            total_rows,
            open: snapshot_fetcher_factory(guard, plan),
        })
    }

    pub(super) fn on_export_all(&self, format: ExportFormat) {
        let selected = self.selected_browse_tab_id();
        let planned = selected
            .and_then(|id| self.export_page_plan(id))
            .zip(selected.and_then(|id| self.tab_connection(id)));
        let (Some(selected), Some(((plan, total_rows), conn))) = (selected, planned) else {
            self.show_toast(&bookie_app_services::tr!("Nothing to export"));
            return;
        };
        let label = match &plan.schema {
            Some(schema) => format!("{schema}.{}", plan.table),
            None => plan.table.clone(),
        };
        let snapshot = self.snapshot_export(selected);
        let driver_id = plan.driver_id.clone();
        let columns_only = QueryResult {
            columns: plan.columns.clone(),
            rows: Vec::new().into(),
            truncated: false,
        };
        crate::ui::export_dialog::present_with_format(
            &self.window,
            &self.toast_overlay,
            crate::ui::export_dialog::ExportRequest {
                scope: if snapshot.is_some() {
                    crate::ui::export_dialog::ExportScope::TableSnapshot {
                        filtered: !plan.filter.is_empty(),
                    }
                } else {
                    crate::ui::export_dialog::ExportScope::TablePage {
                        filtered: !plan.filter.is_empty(),
                    }
                },
                paged: Some(snapshot.unwrap_or_else(|| crate::ui::export_dialog::PagedExport {
                    mode: crate::ui::export_dialog::PagedExportMode::FullTable,
                    total_rows,
                    open: page_fetcher_factory(conn, plan),
                })),
                result: std::sync::Arc::new(columns_only),
                suggested_name: label,
                driver_id,
            },
            matches!(format, ExportFormat::Json),
            &self.preferences,
        );
    }

    /// Ctrl+F / Filter button — toggle the inline filter strip on
    /// the active Browse tab. Strip lives inside the tab (always
    /// constructed at init), so this is just a reveal flip.
    pub(super) fn on_show_filter_dialog(&self) {
        if let Some(id) = self.selected_editor_tab_id() {
            self.dispatch_to_editor(id, crate::ui::editor::SqlEditorInput::ShowFind);
            return;
        }
        let Some(id) = self.selected_browse_tab_id() else {
            self.show_toast(&bookie_app_services::tr!("Open a table to filter rows."));
            return;
        };
        self.dispatch_to_tab(id, BrowseTabInput::ToggleFilterStrip);
    }

    pub(super) fn on_refresh_active_tab(&self) {
        let Some(id) = self.selected_browse_tab_id() else {
            return;
        };
        let save_in_flight = self
            .workspace_tabs
            .borrow()
            .get(&id)
            .and_then(|tab| tab.browse_controller())
            .is_some_and(|controller| controller.model().save_in_flight());
        let dirty = self
            .change_trackers
            .with_tab_ref(id, |tr| tr.has_pending())
            .unwrap_or(false);
        match refresh_decision(dirty, save_in_flight) {
            RefreshDecision::Refresh => {
                self.dispatch_to_tab(id, BrowseTabInput::Refresh);
                return;
            }
            RefreshDecision::ConfirmDiscard => {}
            RefreshDecision::SaveInFlight => {
                self.show_toast(&bookie_app_services::tr!(
                    "Saving changes. Refresh after the save finishes."
                ));
                return;
            }
        }
        let dialog = adw::AlertDialog::new(
            Some(&bookie_app_services::tr!("Discard pending changes?")),
            Some(&bookie_app_services::tr!(
                "Refreshing reloads the table from the database and drops every unsaved edit on this tab."
            )),
        );
        dialog.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        dialog.add_response("discard", &bookie_app_services::tr!("Discard and refresh"));
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let workspace_tabs = self.workspace_tabs.clone();
        let change_trackers = self.change_trackers.clone();
        connect_refresh_response(&dialog, move || {
            change_trackers.with_tab(id, |t| t.clear());
            if let Some(controller) = workspace_tabs.borrow().get(&id).and_then(|t| t.browse_controller()) {
                let _ = controller.sender().send(BrowseTabInput::Refresh);
            }
        });
        dialog.present(Some(&self.window));
    }
}

enum FrontExport {
    Browse,
    Editor {
        result: std::sync::Arc<QueryResult>,
        name: String,
    },
    Empty,
}

fn connect_refresh_response(dialog: &adw::AlertDialog, on_discard: impl Fn() + 'static) {
    dialog.connect_response(None, move |_, response| {
        if response == "discard" {
            on_discard();
        }
    });
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RefreshDecision {
    Refresh,
    ConfirmDiscard,
    SaveInFlight,
}

pub(super) fn refresh_decision(dirty: bool, save_in_flight: bool) -> RefreshDecision {
    if save_in_flight {
        return RefreshDecision::SaveInFlight;
    }
    if dirty {
        RefreshDecision::ConfirmDiscard
    } else {
        RefreshDecision::Refresh
    }
}

fn columns_read_is_current(request: &tokio_util::sync::CancellationToken) -> bool {
    !request.is_cancelled()
}

fn browse_cell_value_query(
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    driver_id: &str,
    col_index: usize,
    column_name: &str,
    row_key: &[bookie_core::Value],
) -> Result<crate::services::browse_query::BoundQuery, String> {
    if columns.get(col_index).is_none_or(|column| column.name != column_name) {
        return Err(
            bookie_app_services::tr!("The table columns changed before the value could be fetched.").to_string(),
        );
    }
    BrowseTarget {
        driver_id,
        schema,
        table,
        columns,
        filter: &bookie_core::FilterSet::default(),
        hidden_columns: None,
        long_text_preview: false,
    }
    .value_query(col_index, row_key)
}

fn single_cell_value(result: QueryResult) -> Result<bookie_core::Value, String> {
    if result.columns.len() != 1 || result.truncated {
        return Err(bookie_app_services::tr!("The row key did not identify exactly one cell.").to_string());
    }
    let Some(row) = result.rows.first() else {
        return Err(bookie_app_services::tr!("The row key did not identify exactly one cell.").to_string());
    };
    if result.rows.len() != 1 {
        return Err(bookie_app_services::tr!("The row key did not identify exactly one cell.").to_string());
    }
    let [value] = row else {
        return Err(bookie_app_services::tr!("The row key did not identify exactly one cell.").to_string());
    };
    Ok(value.clone())
}

fn row_count_from_result(result: &QueryResult) -> Option<u64> {
    let value = result.rows.first()?.first()?;
    match value {
        bookie_core::Value::Int(i) if *i >= 0 => Some(*i as u64),
        bookie_core::Value::Float(f) if *f >= 0.0 && f.is_finite() => Some(*f as u64),
        bookie_core::Value::Decimal(d) => d.to_string().parse::<u64>().ok(),
        _ => None,
    }
}

async fn run_browse_count(
    connection: std::sync::Arc<dyn bookie_core::Connection>,
    query: crate::services::browse_query::BoundQuery,
    control: &bookie_core::OperationControl,
) -> Result<QueryResult, bookie_core::DriverError> {
    if query.params.is_empty() {
        connection.query_controlled(&query.sql, control).await
    } else {
        connection
            .query_params_controlled(&query.sql, &query.params, control)
            .await
    }
}

fn snapshot_fetcher_factory(
    guard: std::sync::Arc<bookie_policy::PolicyGuard>,
    plan: crate::services::export_pages::PagePlan,
) -> crate::ui::export_dialog::PageFetcherFactory {
    let run = page_runner();
    std::sync::Arc::new(move |cancel| {
        let pages =
            bookie_app_services::snapshot_export::SnapshotPages::new(guard.clone(), plan.clone(), run.clone(), cancel);
        let shared = std::sync::Arc::new(std::sync::Mutex::new(Some(pages)));
        let reader = shared.clone();
        crate::ui::export_dialog::PageFetcher::new(
            move || {
                let mut slot = reader.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                match slot.as_mut() {
                    Some(pages) => pages.next_page(),
                    None => Ok(None),
                }
            },
            move || {
                let taken = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
                taken.map_or(Ok(()), |pages| pages.finish())
            },
        )
    })
}

fn page_runner() -> crate::services::export_pages::PageRunner {
    std::sync::Arc::new(|future| {
        let (reply, answer) = async_channel::bounded(1);
        relm4::spawn(async move {
            let _ = reply.send(future.await).await;
        });
        answer.recv_blocking().unwrap_or_else(|_| {
            Err(bookie_core::export::ExportError::Source(
                "the export was interrupted".into(),
            ))
        })
    })
}

fn page_fetcher_factory(
    conn: std::sync::Arc<dyn bookie_core::Connection>,
    plan: crate::services::export_pages::PagePlan,
) -> crate::ui::export_dialog::PageFetcherFactory {
    let run = page_runner();
    std::sync::Arc::new(move |cancel| {
        let fetcher = crate::services::export_pages::page_fetcher(conn.clone(), plan.clone(), run.clone(), cancel);
        crate::ui::export_dialog::PageFetcher::new(fetcher, || Ok(()))
    })
}

#[cfg(test)]
mod tests {
    use relm4::adw::prelude::*;

    use super::{
        RefreshDecision, browse_cell_value_query, columns_read_is_current, connect_refresh_response, refresh_decision,
        row_count_from_result, single_cell_value,
    };
    use crate::services::read_scopes::{ReadScope, ReadScopes};
    use bookie_core::{ColumnInfo, QueryResult, Value};
    use uuid::Uuid;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn force_dismissing_refresh_confirmation_does_not_abort() {
        relm4::adw::init().unwrap();
        relm4::gtk::gio::resources_register_include!("bookie.gresource").unwrap();
        let application = relm4::main_application();
        application.register(None::<&relm4::gtk::gio::Cancellable>).unwrap();
        let window = relm4::adw::ApplicationWindow::new(&application);
        window.present();
        spin_until(|| window.is_mapped());
        let dialog = relm4::adw::AlertDialog::new(None, None);
        dialog.set_close_response("cancel");
        let discarded = std::rc::Rc::new(std::cell::Cell::new(false));
        let discarded_for_response = discarded.clone();
        connect_refresh_response(&dialog, move || discarded_for_response.set(true));
        let closed = std::rc::Rc::new(std::cell::Cell::new(false));
        let closed_for_signal = closed.clone();
        dialog.connect_closed(move |_| closed_for_signal.set(true));
        dialog.present(Some(&window));
        spin_until(|| dialog.is_mapped());

        dialog.force_close();
        spin_until(|| closed.get());

        assert!(!discarded.get());
    }

    fn spin_until(mut condition: impl FnMut() -> bool) {
        let context = relm4::gtk::glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !condition() && std::time::Instant::now() < deadline {
            context.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(condition());
    }

    #[test]
    fn a_late_column_read_after_a_newer_schema_refresh_is_discarded() {
        let mut reads = ReadScopes::default();
        let tab = Uuid::new_v4();
        let old = reads.start(tab, ReadScope::Columns);
        let current = reads.start(tab, ReadScope::Columns);

        assert!(!columns_read_is_current(&old));
        assert!(columns_read_is_current(&current));
    }

    #[test]
    fn refresh_without_pending_edits_runs_immediately() {
        assert_eq!(refresh_decision(false, false), RefreshDecision::Refresh);
    }

    #[test]
    fn a_dirty_refresh_can_be_confirmed_after_the_save_finishes() {
        assert_eq!(refresh_decision(true, false), RefreshDecision::ConfirmDiscard);
    }

    #[test]
    fn refresh_is_blocked_while_a_save_is_in_flight() {
        assert_eq!(refresh_decision(true, true), RefreshDecision::SaveInFlight);
        assert_eq!(refresh_decision(false, true), RefreshDecision::SaveInFlight);
    }

    fn scalar_result(row: Option<Value>) -> QueryResult {
        QueryResult {
            columns: Vec::new(),
            rows: row.map(|v| vec![vec![v]]).unwrap_or_default().into(),
            truncated: false,
        }
    }

    #[test]
    fn a_non_negative_integer_count_is_used() {
        assert_eq!(row_count_from_result(&scalar_result(Some(Value::Int(42)))), Some(42));
    }

    #[test]
    fn an_empty_result_has_no_count() {
        assert_eq!(row_count_from_result(&scalar_result(None)), None);
    }

    #[test]
    fn a_negative_or_non_finite_count_is_refused_instead_of_wrapping() {
        assert_eq!(row_count_from_result(&scalar_result(Some(Value::Int(-1)))), None);
        assert_eq!(
            row_count_from_result(&scalar_result(Some(Value::Float(f64::NAN)))),
            None
        );
        assert_eq!(row_count_from_result(&scalar_result(Some(Value::Float(-1.0)))), None);
    }

    #[test]
    fn a_non_numeric_scalar_has_no_count() {
        assert_eq!(
            row_count_from_result(&scalar_result(Some(Value::Text("x".into())))),
            None
        );
    }

    #[test]
    fn value_refetch_accepts_only_one_complete_cell() {
        let complete = QueryResult {
            columns: vec![column()],
            rows: vec![vec![Value::Bytes(vec![0, 255])]].into(),
            truncated: false,
        };
        assert_eq!(single_cell_value(complete), Ok(Value::Bytes(vec![0, 255])));

        for result in [
            QueryResult {
                columns: vec![column()],
                rows: Vec::new().into(),
                truncated: false,
            },
            QueryResult {
                columns: vec![column()],
                rows: vec![vec![Value::Int(1)], vec![Value::Int(2)]].into(),
                truncated: false,
            },
            QueryResult {
                columns: Vec::new(),
                rows: vec![vec![Value::Int(1)]].into(),
                truncated: false,
            },
            QueryResult {
                columns: vec![column()],
                rows: vec![vec![Value::Int(1)]].into(),
                truncated: true,
            },
        ] {
            assert!(single_cell_value(result).is_err());
        }
    }

    #[test]
    fn value_refetch_refuses_a_column_that_changed_since_the_request() {
        let columns = [column()];
        let error = browse_cell_value_query(None, "records", &columns, "postgres", 0, "old", &[]).unwrap_err();
        assert_eq!(error, "The table columns changed before the value could be fetched.");
    }

    fn column() -> ColumnInfo {
        ColumnInfo {
            name: "payload".into(),
            data_type: "bytea".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }
}
