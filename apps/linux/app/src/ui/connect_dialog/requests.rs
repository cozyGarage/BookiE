use super::bootstrap::ConnectRequest;
use super::*;

struct Resolved {
    entry: DriverEntry,
    driver: Arc<dyn bookie_core::DatabaseDriver>,
    opts: ConnectOptions,
    ssh: Option<SshInputs>,
}

impl ConnectDialog {
    fn resolve_target(&self) -> Result<Resolved, String> {
        let entry = self
            .drivers
            .get(self.driver_combo.selected() as usize)
            .cloned()
            .ok_or_else(|| bookie_app_services::tr!("No driver selected"))?;
        let driver = self
            .registry
            .get(&entry.id)
            .ok_or_else(|| bookie_app_services::tr!("Driver {id} not registered").replace("{id}", &entry.id))?;
        let opts = self.collect_options(driver.as_ref());
        let ssh = if self.ssh.is_enabled() {
            Some(self.ssh.collect()?)
        } else {
            None
        };
        Ok(Resolved {
            entry,
            driver,
            opts,
            ssh,
        })
    }

    fn start_request(&mut self, resolved: Resolved) -> ConnectRequest {
        let Resolved {
            entry,
            driver,
            opts,
            ssh,
        } = resolved;
        let query_timeout_secs = timeout_value(self.query_timeout.value());
        let timeout_secs = query_timeout_secs
            .unwrap_or_else(|| crate::services::operation_control::configured_timeout_secs(&self.preferences));
        let cancel = tokio_util::sync::CancellationToken::new();
        self.connect_cancel = Some(cancel.clone());
        ConnectRequest {
            driver,
            label: saved_connection_name(self.name.text().as_str(), &entry.id, &opts),
            driver_id: entry.id,
            bound_id: self.bound_connection_id,
            hidden_schemas: self.hidden_schemas.clone(),
            opts,
            ssh,
            ssh_environment: self.ssh_environment.clone(),
            audit_factory: self.audit_factory.clone(),
            guard_factory: self.guard_factory.clone(),
            workspace: self.workspace.clone(),
            read_only: self.read_only.is_active(),
            environment: self.selected_environment(),
            timeout_secs,
            query_timeout_secs,
            cancel,
        }
    }

    fn begin(&mut self, kind: BusyKind) -> Option<ConnectRequest> {
        self.set_busy(kind);
        match self.resolve_target() {
            Ok(resolved) => Some(self.start_request(resolved)),
            Err(message) => {
                self.set_busy(BusyKind::None);
                self.show_toast(&message);
                None
            }
        }
    }

    pub(super) fn submit(&mut self, sender: &ComponentSender<Self>) {
        if let Some(cancel) = self.connect_cancel.take() {
            cancel.cancel();
            return;
        }
        let Some(request) = self.begin(BusyKind::Connecting) else {
            return;
        };
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let message = match run_connect(request).await {
                        Ok(Some(prepared)) => ConnectDialogCmd::Result(Ok(Box::new(prepared))),
                        Ok(None) => ConnectDialogCmd::Cancelled,
                        Err(error) => ConnectDialogCmd::Result(Err(error)),
                    };
                    out.send(message).ok();
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn test_connection(&mut self, sender: &ComponentSender<Self>) {
        self.test_banner.set_revealed(false);
        let Some(request) = self.begin(BusyKind::Testing) else {
            return;
        };
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    out.send(ConnectDialogCmd::TestResult(run_test(request).await)).ok();
                })
                .drop_on_shutdown()
        });
    }
}

async fn run_test(request: ConnectRequest) -> Result<usize, String> {
    let ConnectRequest {
        driver,
        driver_id,
        bound_id,
        hidden_schemas,
        label,
        opts,
        ssh,
        ssh_environment,
        audit_factory,
        guard_factory,
        workspace,
        read_only,
        environment,
        timeout_secs,
        query_timeout_secs,
        cancel,
    } = request;
    let route = SshInputs::optional_route(ssh.as_ref(), bound_id).await?;
    let (id, hidden_schemas) = locate_saved(
        &*driver,
        &driver_id,
        (bound_id, &opts, ssh.as_ref()),
        &workspace,
        hidden_schemas,
    )
    .await;
    let audit = audit_factory.context(
        bookie_policy::Principal::human_gui(),
        id,
        label.clone(),
        environment,
        driver_id.clone(),
    );
    let ssh_environment = ssh_environment.with_audit(audit);
    let metadata = crate::services::database_service::ConnectionMetadata {
        id,
        name: label,
        driver_id,
        environment,
        read_only,
        server_version: None,
        query_timeout_secs,
    };
    let (conn, _tunnel) =
        bookie_transport::establish_with_cancellation(driver.as_ref(), opts, route, &ssh_environment, cancel)
            .await
            .map_err(crate::services::error_text::transport_message)?;
    let conn: Arc<dyn bookie_core::Connection> = Arc::from(conn);
    let control = crate::services::operation_control::bounded(timeout_secs);
    let guarded = guard_factory
        .guard(conn, &metadata, bookie_policy::Principal::human_gui())
        .map_err(|error| policy_denied_text(&error))?;
    crate::services::catalog::list_tables_excluding(&guarded, &hidden_schemas, &control)
        .await
        .map(|tables| tables.len())
        .map_err(|error| crate::services::error_text::driver_message(&error))
}

fn stored_hidden_schemas(
    existing: Option<&SavedConnection>,
    workspace: &crate::services::workspace_state::WorkspaceStore,
) -> Option<Vec<String>> {
    let saved = existing?;
    workspace
        .load_database(saved.id, &saved.database)
        .map(|state| state.hidden_schemas)
}

fn policy_denied_text(failure: &bookie_app_services::open_failure::OpenFailure) -> String {
    crate::services::error_text::driver_message(&bookie_core::DriverError::PolicyDenied(
        crate::services::error_text::open_failure_message(failure),
    ))
}

async fn locate_saved(
    driver: &dyn bookie_core::DatabaseDriver,
    driver_id: &str,
    (bound_id, opts, ssh): (Option<Uuid>, &ConnectOptions, Option<&SshInputs>),
    workspace: &crate::services::workspace_state::WorkspaceStore,
    fallback_hidden: Vec<String>,
) -> (Uuid, Vec<String>) {
    let identity = ConnectionIdentity {
        bound_id,
        driver_id,
        opts,
        file_based: driver.capabilities().is_file_based,
        ssh,
    };
    let existing = find_existing(&identity).await;
    let id = existing
        .as_ref()
        .map_or_else(|| bound_id.unwrap_or_else(Uuid::new_v4), |saved| saved.id);
    let hidden = stored_hidden_schemas(existing.as_ref(), workspace).unwrap_or(fallback_hidden);
    (id, hidden)
}
