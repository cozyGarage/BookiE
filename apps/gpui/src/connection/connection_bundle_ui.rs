use bookie_app_services::service_runtime::block_on;
use std::path::{Path, PathBuf};

use crate::surfaces::{ButtonKind, button, task_bar_frame};
use crate::*;
use bookie_app_services::connection_bundle as bundles;
use gpui_component::scroll::ScrollableElement;

#[derive(Default)]
pub(crate) struct BundleState {
    pub pending: Option<PendingImport>,
    pub encrypted: Option<bundles::EncryptedBundle>,
    pub passphrase: Option<Entity<TextInput>>,
    pub busy: bool,
}

pub(crate) struct PendingImport {
    plan: bundles::ImportPlan,
    body: bundles::BundleBody,
    skipped: Vec<String>,
}

enum PreparedImport {
    Review(PendingImport),
    Locked(bundles::EncryptedBundle),
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "Could not read the selected connection file".to_string())?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(format!(
            "The selected file exceeds the {limit} byte limit or is not a file"
        ));
    }
    let file = std::fs::File::open(path).map_err(|_| "Could not read the selected connection file".to_string())?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    use std::io::Read;
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the selected connection file".to_string())?;
    if bytes.len() > limit {
        return Err(format!("The selected file exceeds the {limit} byte limit"));
    }
    Ok(bytes)
}

fn prepare_import(path: &Path) -> Result<PreparedImport, String> {
    if is_sqlite_file(path)? {
        let imported = block_on(async {
            bundles::parse_dbx_connection_database(path)
                .await
                .map_err(|error| error.to_string())
        })?;
        let skipped = imported.skipped.clone();
        return plan_body(imported.into_body(), skipped);
    }
    let bytes = read_bounded(path, bundles::MAX_BUNDLE_BYTES)?;
    match bundles::parse_bundle(&bytes) {
        Ok(bundles::ParsedBundle::Plaintext(body)) => return plan_body(body, Vec::new()),
        Ok(bundles::ParsedBundle::Encrypted(bundle)) => return Ok(PreparedImport::Locked(bundle)),
        Err(bundles::BundleError::NotABundle | bundles::BundleError::Malformed) => {}
        Err(error) => return Err(error.to_string()),
    }
    let (body, skipped) = parse_peer(path, &bytes)?;
    plan_body(body, skipped)
}

fn is_sqlite_file(path: &Path) -> Result<bool, String> {
    use std::io::Read;
    let mut header = [0; 16];
    let read = std::fs::File::open(path)
        .and_then(|mut file| file.read(&mut header))
        .map_err(|_| "Could not read the selected connection file".to_string())?;
    Ok(read == header.len() && header == *b"SQLite format 3\0")
}

fn parse_peer(path: &Path, bytes: &[u8]) -> Result<(bundles::BundleBody, Vec<String>), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "The connection file is not valid UTF-8".to_string())?;
    let parsed = if text.contains("<Connections") {
        bundles::parse_navicat_connections(text)
    } else if text.trim_start().starts_with('<') {
        let local = read_bounded(
            &path.with_file_name("dataSources.local.xml"),
            bundles::MAX_DATAGRIP_BYTES,
        )
        .ok()
        .and_then(|local| String::from_utf8(local).ok());
        bundles::parse_datagrip_data_sources(text, local.as_deref())
    } else if bundles::looks_like_resp_export(text) {
        bundles::parse_resp_connections(text)
    } else {
        bundles::parse_dbeaver_data_sources(text)
            .or_else(|error| bundles::parse_another_redis_connections(text).map_err(|_| error))
    }
    .map_err(|error| error.to_string())?;
    let skipped = parsed.skipped.clone();
    Ok((parsed.into_body(), skipped))
}

fn plan_body(body: bundles::BundleBody, skipped: Vec<String>) -> Result<PreparedImport, String> {
    let local = block_on(async { bundles::load_connections().await.map_err(|error| error.to_string()) })?;
    let plan = bundles::plan_import(&local, &body).map_err(|error| error.to_string())?;
    Ok(PreparedImport::Review(PendingImport { plan, body, skipped }))
}

fn unlock_import(bundle: &bundles::EncryptedBundle, passphrase: &str) -> Result<PendingImport, String> {
    let body = bundle.unlock(passphrase).map_err(|error| error.to_string())?;
    match plan_body(body, Vec::new())? {
        PreparedImport::Review(pending) => Ok(pending),
        PreparedImport::Locked(_) => Err("The connection bundle could not be unlocked".into()),
    }
}

fn bundle_export(
    connections: Vec<bundles::SavedConnection>,
    organization: Vec<(uuid::Uuid, bundles::ConnectionOrganization)>,
) -> bundles::BundleExport {
    bundles::BundleExport {
        producer: format!("bookie {}", env!("CARGO_PKG_VERSION")),
        exported_at: chrono::Utc::now().to_rfc3339(),
        connections,
        organization,
        secrets: Vec::new(),
    }
}

fn disposition_name(disposition: &bundles::ImportDisposition) -> &'static str {
    match disposition {
        bundles::ImportDisposition::New => "Add",
        bundles::ImportDisposition::UpdateInPlace => "Update",
        bundles::ImportDisposition::Remapped => "Add as new",
    }
}

impl GridView {
    pub(crate) fn pick_connection_import(&mut self, cx: &mut Context<Self>) {
        if self.connection_bundle.busy {
            self.status = "A connection import or export is already running".into();
            return;
        }
        if self.connection_bundle.pending.is_some() || self.connection_bundle.encrypted.is_some() {
            self.status = "Finish or cancel the connection review first".into();
            return;
        }
        self.connection_bundle.busy = true;
        self.status = "Choose a connection file to review".into();
        cx.spawn(async move |this, cx| {
            let selected = rfd::AsyncFileDialog::new()
                .add_filter(
                    "Connection files",
                    &["bookie-connections", "db", "ncx", "json", "xml", "ano"],
                )
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf());
            let Some(path) = selected else {
                this.update(cx, |this, cx| {
                    this.connection_bundle.busy = false;
                    this.status = "Canceled the connection import".into();
                    cx.notify();
                })
                .ok();
                return;
            };
            let outcome = cx
                .background_executor()
                .spawn(async move { prepare_import(&path) })
                .await;
            this.update(cx, |this, cx| {
                this.connection_bundle.busy = false;
                match outcome {
                    Ok(PreparedImport::Review(pending)) if pending.plan.items.is_empty() => {
                        this.status = "The selected file contains no connections to import".into();
                    }
                    Ok(PreparedImport::Review(pending)) => {
                        this.connection_bundle.pending = Some(pending);
                        this.status = "Review the connection changes, then import or cancel".into();
                    }
                    Ok(PreparedImport::Locked(bundle)) => {
                        this.connection_bundle.encrypted = Some(bundle);
                        if this.connection_bundle.passphrase.is_none() {
                            this.connection_bundle.passphrase =
                                Some(cx.new(|cx| TextInput::new("Bundle passphrase", cx).masked()));
                        }
                        this.status = "Enter the bundle passphrase to review its connections".into();
                    }
                    Err(message) => this.report_failure("read connections", &message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn unlock_connection_import(&mut self, cx: &mut Context<Self>) {
        if self.connection_bundle.busy {
            return;
        }
        let Some(passphrase) = self.connection_bundle.passphrase.clone() else {
            return;
        };
        let Some(bundle) = self.connection_bundle.encrypted.take() else {
            return;
        };
        let text = secrecy::SecretString::from(passphrase.read(cx).content().to_string());
        self.connection_bundle.busy = true;
        self.status = "Unlocking the connection bundle…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let result = unlock_import(&bundle, secrecy::ExposeSecret::expose_secret(&text));
                    (bundle, result)
                })
                .await;
            this.update(cx, |this, cx| {
                this.connection_bundle.busy = false;
                match outcome {
                    (bundle, Err(message)) => {
                        this.connection_bundle.encrypted = Some(bundle);
                        this.report_failure("unlock the connection bundle", &message);
                    }
                    (_, Ok(pending)) if pending.plan.items.is_empty() => {
                        this.connection_bundle.encrypted = None;
                        passphrase.update(cx, |input, cx| input.set_content("", cx));
                        this.status = "The selected file contains no connections to import".into();
                    }
                    (_, Ok(pending)) => {
                        this.connection_bundle.encrypted = None;
                        this.connection_bundle.pending = Some(pending);
                        passphrase.update(cx, |input, cx| input.set_content("", cx));
                        this.status = "Review the connection changes, then import or cancel".into();
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn cancel_connection_import(&mut self, cx: &mut Context<Self>) {
        self.connection_bundle.pending = None;
        self.connection_bundle.encrypted = None;
        if let Some(passphrase) = &self.connection_bundle.passphrase {
            passphrase.update(cx, |input, cx| input.set_content("", cx));
        }
        self.status = "Canceled the connection import".into();
        cx.notify();
    }

    pub(crate) fn apply_connection_import(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.connection_bundle.pending.take() else {
            return;
        };
        self.connection_bundle.busy = true;
        self.status = "Importing connections…".into();
        cx.spawn(async move |this, cx| {
            let (result, saved) = cx
                .background_executor()
                .spawn(async move {
                    let result = apply_import(pending);
                    (result, bookie_gpui::live::saved_connections())
                })
                .await;
            this.update(cx, |this, cx| {
                this.connection_bundle.busy = false;
                this.finish_connection_import(result, saved);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn finish_connection_import(
        &mut self,
        result: Result<usize, String>,
        saved: Result<Vec<bookie_app_services::read_session::ConnectionSummary>, String>,
    ) {
        use crate::notifications::NoticeKind;
        let kind = if result.is_ok() && saved.is_ok() {
            NoticeKind::Success
        } else {
            NoticeKind::Error
        };
        match (result, saved) {
            (Ok(count), Ok(connections)) => {
                self.connections = connections;
                self.status = format!("Imported {count} connection(s)");
            }
            (Err(message), Ok(connections)) => {
                self.connections = connections;
                self.status = message;
            }
            (Err(message), Err(_)) => self.status = message,
            (Ok(_), Err(_)) => {
                self.status = "Connections were imported, but the list could not be refreshed".into()
            }
        }
        self.notices.push(self.status.clone(), kind);
    }

    pub(crate) fn pick_connection_export(&mut self, cx: &mut Context<Self>) {
        if self.connection_bundle.busy {
            self.status = "A connection import or export is already running".into();
            return;
        }
        self.connection_bundle.busy = true;
        self.status = "Choose where to save the connection bundle".into();
        cx.spawn(async move |this, cx| {
            let selected = rfd::AsyncFileDialog::new()
                .set_file_name(&format!(
                    "bookie-connections-{}.bookie-connections",
                    chrono::Utc::now().format("%Y-%m-%d")
                ))
                .add_filter("BookiE connection bundle", &["bookie-connections"])
                .save_file()
                .await
                .map(|file| file.path().to_path_buf());
            let Some(path) = selected else {
                this.update(cx, |this, cx| {
                    this.connection_bundle.busy = false;
                    this.status = "Canceled the connection export".into();
                    cx.notify();
                })
                .ok();
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { export_connections(path) })
                .await;
            this.update(cx, |this, cx| {
                this.connection_bundle.busy = false;
                match result {
                    Ok(count) => {
                        this.status = format!("Exported {count} connection(s); passwords and SSH secrets were omitted")
                    }
                    Err(message) => this.report_failure("export connections", &message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn connection_bundle_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        if self.connection_bundle.busy {
            return None;
        }
        if let Some(pending) = &self.connection_bundle.pending {
            let rows = pending.plan.items.iter().map(|item| {
                div()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(item.connection.name.clone()),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme.muted_foreground))
                            .child(disposition_name(&item.disposition)),
                    )
            });
            let count = pending.plan.items.len();
            let skipped = pending.skipped.len();
            return Some(
                task_bar_frame(theme)
                    .w_full()
                    .debug_selector(|| "connection-import-review".into())
                    .items_start()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .max_h(px(crate::size::SAVE_PREVIEW_MAX_HEIGHT))
                            .overflow_y_scrollbar()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("Review {count} connection(s)")),
                            )
                            .children(rows)
                            .when(skipped > 0, |view| {
                                view.child(
                                    div()
                                        .text_color(rgb(theme.muted_foreground))
                                        .child(format!("{skipped} unsupported connection(s) will be skipped")),
                                )
                                .children(pending.skipped.iter().map(|name| {
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_color(rgb(theme.muted_foreground))
                                        .child(name.clone())
                                }))
                            }),
                    )
                    .child(
                        button(ButtonKind::Primary, "confirm-connection-import", "Import", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.apply_connection_import(cx))),
                    )
                    .child(
                        button(ButtonKind::Ghost, "cancel-connection-import", "Cancel", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_connection_import(cx))),
                    ),
            );
        }
        if self.connection_bundle.encrypted.is_some() {
            let input = self.connection_bundle.passphrase.clone()?;
            return Some(
                task_bar_frame(theme)
                    .child(div().flex_1().child(input))
                    .child(
                        button(ButtonKind::Primary, "unlock-connection-import", "Review", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.unlock_connection_import(cx))),
                    )
                    .child(
                        button(ButtonKind::Ghost, "cancel-connection-import", "Cancel", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_connection_import(cx))),
                    ),
            );
        }
        None
    }
}

fn plan_is_current(plan: &bundles::ImportPlan, local: &[bundles::SavedConnection]) -> bool {
    plan.items
        .iter()
        .all(|item| local.iter().find(|saved| saved.id == item.connection.id) == item.previous.as_ref())
}

fn apply_import(pending: PendingImport) -> Result<usize, String> {
    block_on(async move {
        let local = bundles::load_connections()
            .await
            .map_err(|_| "The saved connections could not be read, so nothing was imported".to_string())?;
        if !plan_is_current(&pending.plan, &local) {
            return Err(
                "Saved connections changed after this import was reviewed, so nothing was imported. Choose the file again to review the current changes"
                    .into(),
            );
        }
        let database = bookie_app_services::database_service::DatabaseService::new();
        let operation = uuid::Uuid::new_v4();
        let planned = pending.plan.items.len();
        database
            .audit_administrative_action(
                operation,
                bundles::AuditAdministrativeAction::ConnectionBundleImport,
                bundles::AuditRecordPhase::Intent,
                bundles::AuditTerminalStatus::Pending,
                planned,
                None,
            )
            .await
            .map_err(|_| "Canceled the import because its audit record could not be saved".to_string())?;
        let started = Instant::now();
        let outcome = apply_import_body(pending.body, pending.plan).await;
        let (status, affected) = match &outcome {
            Ok(count) => (bundles::AuditTerminalStatus::Succeeded, *count),
            Err(_) => (bundles::AuditTerminalStatus::Failed, 0),
        };
        if database
            .audit_administrative_action(
                operation,
                bundles::AuditAdministrativeAction::ConnectionBundleImport,
                bundles::AuditRecordPhase::Outcome,
                status,
                affected,
                Some(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
            )
            .await
            .is_err()
        {
            return Err("The import finished, but its audit outcome could not be saved".into());
        }
        outcome.map_err(|error| format!("The connection import was not applied: {error}"))
    })
}

async fn apply_import_body(body: bundles::BundleBody, reviewed: bundles::ImportPlan) -> Result<usize, String> {
    Box::pin(bundles::apply_reviewed_import_body(&body, &reviewed))
        .await
        .map(|plan| plan.items.len())
        .map_err(|error| error.to_string())
}

fn export_connections(path: PathBuf) -> Result<usize, String> {
    block_on(async move {
        let database = bookie_app_services::database_service::DatabaseService::new();
        let operation = uuid::Uuid::new_v4();
        let connections = bundles::load_connections().await.map_err(|error| error.to_string())?;
        if connections.is_empty() {
            return Err("There are no saved connections to export".into());
        }
        database
            .audit_administrative_action(
                operation,
                bundles::AuditAdministrativeAction::ConnectionBundleExport,
                bundles::AuditRecordPhase::Intent,
                bundles::AuditTerminalStatus::Pending,
                connections.len(),
                None,
            )
            .await
            .map_err(|_| "Canceled the export because its audit record could not be saved".to_string())?;
        let started = Instant::now();
        let outcome = write_export(path, connections).await;
        let status = if outcome.is_ok() {
            bundles::AuditTerminalStatus::Succeeded
        } else {
            bundles::AuditTerminalStatus::Failed
        };
        let count = outcome.as_ref().copied().unwrap_or_default();
        if database
            .audit_administrative_action(
                operation,
                bundles::AuditAdministrativeAction::ConnectionBundleExport,
                bundles::AuditRecordPhase::Outcome,
                status,
                count,
                Some(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
            )
            .await
            .is_err()
        {
            return Err("The export finished, but its audit outcome could not be saved".into());
        }
        outcome.map_err(|error| error.to_string())
    })
}

async fn write_export(
    path: PathBuf,
    connections: Vec<bundles::SavedConnection>,
) -> Result<usize, bundles::BundleError> {
    let count = connections.len();
    let organization = bundles::load_organization()
        .await
        .map_err(|_| bundles::BundleError::Field("organization"))?;
    let organization = connections
        .iter()
        .map(|connection| (connection.id, organization.get(connection.id)))
        .collect();
    let export = bundle_export(connections, organization);
    let bytes = bundles::export_plaintext(&export)?;
    bookie_core::export::write_atomically(&path, |file| file.write_all(&bytes))
        .map_err(|_| bundles::BundleError::Field("destination"))?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn import_outcomes_reach_notifications_with_the_right_severity(cx: &mut gpui::TestAppContext) {
        use crate::notifications::NoticeKind;
        let (view, cx) = crate::ui_tests::open(cx);
        for (result, saved, kind) in [
            (Ok(2), Ok(vec![]), NoticeKind::Success),
            (Err("Import failed".to_string()), Ok(vec![]), NoticeKind::Error),
            (Err("Import failed".to_string()), Err("Refresh failed".to_string()), NoticeKind::Error),
            (Ok(2), Err("Refresh failed".to_string()), NoticeKind::Error),
        ] {
            view.update(cx, |view, _| {
                let before = view.notices.items.len();
                view.finish_connection_import(result, saved);
                assert_eq!(view.notices.items.len(), before + 1);
                assert_eq!(view.notices.items[0].kind, kind);
                assert_eq!(view.notices.items[0].text, view.status);
            });
        }
    }

    #[test]
    fn import_disposition_text_is_user_visible_and_distinct() {
        assert_ne!(
            disposition_name(&bundles::ImportDisposition::New),
            disposition_name(&bundles::ImportDisposition::UpdateInPlace)
        );
        assert_ne!(
            disposition_name(&bundles::ImportDisposition::Remapped),
            disposition_name(&bundles::ImportDisposition::UpdateInPlace)
        );
    }

    #[test]
    fn import_menu_auto_detects_dbeaver_and_keeps_unsupported_names_for_review() -> Result<(), String> {
        let input = br#"{"connections":{"pg":{"provider":"postgresql","name":"local","configuration":{"host":"localhost","port":"5432","database":"test"}},"oracle":{"provider":"oracle","name":"legacy"}}}"#;
        let (body, skipped) = parse_peer(Path::new("connections.json"), input)?;
        assert_eq!(body.connections.len(), 1);
        assert_eq!(body.connections[0].name, "local");
        assert_eq!(skipped, ["legacy"]);
        Ok(())
    }

    #[test]
    fn bounded_reader_rejects_a_file_over_its_limit() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!("bookie-import-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"four")?;
        let result = read_bounded(&path, 3);
        std::fs::remove_file(path)?;
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn invalid_connection_file_is_refused_before_import_review() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!("bookie-import-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"not a supported connection file")?;
        let result = prepare_import(&path);
        std::fs::remove_file(path)?;
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn exported_bundle_contains_profiles_but_no_secrets() -> Result<(), Box<dyn std::error::Error>> {
        let connection: bundles::SavedConnection = serde_json::from_str(
            r#"{"id":"6af4b280-82ec-4d81-b780-3878f7af1647","name":"local","driver_id":"postgres","host":"localhost","port":5432,"database":"test","username":"test"}"#,
        )?;
        let bytes = bundles::export_plaintext(&bundle_export(vec![connection], Vec::new()))?;
        let bundles::ParsedBundle::Plaintext(body) = bundles::parse_bundle(&bytes)? else {
            return Err("plaintext export was unexpectedly encrypted".into());
        };
        assert_eq!(body.connections.len(), 1);
        assert!(body.secrets.is_empty());
        Ok(())
    }

    #[test]
    fn an_import_reviewed_before_a_local_change_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let saved: bundles::SavedConnection = serde_json::from_str(
            r#"{"id":"6af4b280-82ec-4d81-b780-3878f7af1647","name":"prod","driver_id":"postgres","host":"db","port":5432,"database":"app","username":"app"}"#,
        )?;
        let bytes = bundles::export_plaintext(&bundle_export(vec![saved.clone()], Vec::new()))?;
        let bundles::ParsedBundle::Plaintext(body) = bundles::parse_bundle(&bytes)? else {
            return Err("plaintext export was unexpectedly encrypted".into());
        };
        let plan = bundles::plan_import(std::slice::from_ref(&saved), &body)?;
        assert!(plan_is_current(&plan, std::slice::from_ref(&saved)));

        let mut changed = saved;
        changed.name = "prod (locked down)".into();
        assert!(!plan_is_current(&plan, std::slice::from_ref(&changed)));
        assert!(!plan_is_current(&plan, &[]));
        Ok(())
    }

    #[gpui::test]
    fn import_review_actions_remain_visible_on_home_and_connection_tabs(cx: &mut gpui::TestAppContext) {
        let (view, cx) = crate::ui_tests::open(cx);
        cx.simulate_resize(gpui::size(gpui::px(1100.0), gpui::px(800.0)));
        let connection: bundles::SavedConnection = serde_json::from_str(
            r#"{"id":"6af4b280-82ec-4d81-b780-3878f7af1647","name":"Review fixture","driver_id":"postgres","host":"localhost","port":5432,"database":"test","username":"test"}"#,
        )
        .expect("valid review fixture");
        let bytes =
            bundles::export_plaintext(&bundle_export(vec![connection], Vec::new())).expect("fixture serializes");
        let bundles::ParsedBundle::Plaintext(body) = bundles::parse_bundle(&bytes).expect("fixture parses") else {
            panic!("fixture must be a plaintext bundle");
        };
        let pending = PendingImport {
            plan: bundles::plan_import(&[], &body).expect("valid review fixture"),
            body,
            skipped: Vec::new(),
        };

        cx.update(|_, cx| {
            view.update(cx, |this, cx| {
                this.open_connection_dialog(cx);
                this.connection_bundle.pending = Some(pending);
                cx.notify();
            });
        });
        cx.run_until_parked();
        view.read_with(cx, |this, _| {
            assert!(this.connection_bundle.pending.is_some());
            assert!(!this.connection_bundle.busy);
            assert!(this.connection_form.open);
        });
        assert!(cx.debug_bounds("connection-page").is_some());
        assert!(
            cx.debug_bounds("connection-import-review").is_some(),
            "the import confirmation must remain visible on the New Connection tab"
        );

        cx.update(|_, cx| {
            view.update(cx, |this, cx| {
                this.tabs.open_home();
                cx.notify();
            });
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("connection-import-review").is_some(),
            "the import confirmation must remain visible on the Home tab"
        );
    }
}
