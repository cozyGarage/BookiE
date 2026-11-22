use std::path::{Path, PathBuf};

use super::*;
use crate::chrome::ghost;
use gpui_component::scroll::ScrollableElement;

#[derive(Default)]
pub(crate) struct BundleState {
    pub pending: Option<PendingImport>,
    pub encrypted: Option<bookie_storage::EncryptedBundle>,
    pub passphrase: Option<Entity<TextInput>>,
    pub busy: bool,
}

pub(crate) struct PendingImport {
    plan: bookie_storage::ImportPlan,
    secrets: Vec<bookie_storage::BundleSecrets>,
    skipped: Vec<String>,
}

enum PreparedImport {
    Review(PendingImport),
    Locked(bookie_storage::EncryptedBundle),
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
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
        let imported = runtime()?
            .block_on(bookie_storage::parse_dbx_connection_database(path))
            .map_err(|error| error.to_string())?;
        let skipped = imported.skipped.clone();
        return plan_body(imported.into_body(), skipped);
    }
    let bytes = read_bounded(path, bookie_storage::MAX_BUNDLE_BYTES)?;
    match bookie_storage::parse_bundle(&bytes) {
        Ok(bookie_storage::ParsedBundle::Plaintext(body)) => return plan_body(body, Vec::new()),
        Ok(bookie_storage::ParsedBundle::Encrypted(bundle)) => return Ok(PreparedImport::Locked(bundle)),
        Err(bookie_storage::BundleError::NotABundle | bookie_storage::BundleError::Malformed) => {}
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

fn parse_peer(path: &Path, bytes: &[u8]) -> Result<(bookie_storage::BundleBody, Vec<String>), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "The connection file is not valid UTF-8".to_string())?;
    let parsed = if text.contains("<Connections") {
        bookie_storage::parse_navicat_connections(text)
    } else if text.trim_start().starts_with('<') {
        let local = read_bounded(
            &path.with_file_name("dataSources.local.xml"),
            bookie_storage::MAX_DATAGRIP_BYTES,
        )
        .ok()
        .and_then(|local| String::from_utf8(local).ok());
        bookie_storage::parse_datagrip_data_sources(text, local.as_deref())
    } else if bookie_storage::looks_like_resp_export(text) {
        bookie_storage::parse_resp_connections(text)
    } else {
        bookie_storage::parse_dbeaver_data_sources(text)
            .or_else(|error| bookie_storage::parse_another_redis_connections(text).map_err(|_| error))
    }
    .map_err(|error| error.to_string())?;
    let skipped = parsed.skipped.clone();
    Ok((parsed.into_body(), skipped))
}

fn plan_body(body: bookie_storage::BundleBody, skipped: Vec<String>) -> Result<PreparedImport, String> {
    let local = runtime()?
        .block_on(bookie_storage::load_connections())
        .map_err(|error| error.to_string())?;
    let plan = bookie_storage::plan_import(&local, &body).map_err(|error| error.to_string())?;
    Ok(PreparedImport::Review(PendingImport {
        plan,
        secrets: body.secrets,
        skipped,
    }))
}

fn unlock_import(bundle: &bookie_storage::EncryptedBundle, passphrase: &str) -> Result<PendingImport, String> {
    let body = bundle.unlock(passphrase).map_err(|error| error.to_string())?;
    match plan_body(body, Vec::new())? {
        PreparedImport::Review(pending) => Ok(pending),
        PreparedImport::Locked(_) => Err("The connection bundle could not be unlocked".into()),
    }
}

fn bundle_export(
    connections: Vec<bookie_storage::SavedConnection>,
    organization: Vec<(uuid::Uuid, bookie_storage::ConnectionOrganization)>,
) -> bookie_storage::BundleExport {
    bookie_storage::BundleExport {
        producer: format!("bookie {}", env!("CARGO_PKG_VERSION")),
        exported_at: chrono::Utc::now().to_rfc3339(),
        connections,
        organization,
        secrets: Vec::new(),
    }
}

fn disposition_name(disposition: &bookie_storage::ImportDisposition) -> &'static str {
    match disposition {
        bookie_storage::ImportDisposition::New => "Add",
        bookie_storage::ImportDisposition::UpdateInPlace => "Update",
        bookie_storage::ImportDisposition::Remapped => "Add as new",
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
                    this.status = "Cancelled the connection import".into();
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
                    Err(message) => this.status = format!("Could not read connections: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn unlock_connection_import(&mut self, cx: &mut Context<Self>) {
        let (Some(bundle), Some(passphrase)) = (
            self.connection_bundle.encrypted.as_ref(),
            self.connection_bundle.passphrase.clone(),
        ) else {
            return;
        };
        let text = secrecy::SecretString::from(passphrase.read(cx).content().to_string());
        match unlock_import(bundle, secrecy::ExposeSecret::expose_secret(&text)) {
            Ok(pending) if pending.plan.items.is_empty() => {
                self.connection_bundle.encrypted = None;
                self.status = "The selected file contains no connections to import".into();
            }
            Ok(pending) => {
                self.connection_bundle.encrypted = None;
                self.connection_bundle.pending = Some(pending);
                passphrase.update(cx, |input, cx| input.set_content("", cx));
                self.status = "Review the connection changes, then import or cancel".into();
            }
            Err(message) => self.status = format!("Could not unlock the connection bundle: {message}"),
        }
        cx.notify();
    }

    pub(crate) fn cancel_connection_import(&mut self, cx: &mut Context<Self>) {
        self.connection_bundle.pending = None;
        self.connection_bundle.encrypted = None;
        self.status = "Cancelled the connection import".into();
        cx.notify();
    }

    pub(crate) fn apply_connection_import(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.connection_bundle.pending.take() else {
            return;
        };
        self.connection_bundle.busy = true;
        self.status = "Importing connections…".into();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { apply_import(pending) })
                .await;
            let saved = bookie_gpui::live::saved_connections();
            this.update(cx, |this, cx| {
                this.connection_bundle.busy = false;
                match (result, saved) {
                    (Ok(count), Ok(connections)) => {
                        this.connections = connections;
                        this.status = format!("Imported {count} connection(s)");
                    }
                    (Err(message), Ok(connections)) => {
                        this.connections = connections;
                        this.status = message;
                    }
                    (Err(message), Err(_)) => this.status = message,
                    (Ok(_), Err(_)) => {
                        this.status = "Connections were imported, but the list could not be refreshed".into()
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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
                    this.status = "Cancelled the connection export".into();
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
                this.status = match result {
                    Ok(count) => format!("Exported {count} connection(s); passwords and SSH secrets were omitted"),
                    Err(message) => format!("Could not export connections: {message}"),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn connection_bundle_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
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
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .p_2()
                    .flex_none()
                    .bg(rgb(theme.chrome))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .max_h(px(180.0))
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
                        ghost("confirm-connection-import", "Import", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.apply_connection_import(cx))),
                    )
                    .child(
                        ghost("cancel-connection-import", "Cancel", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_connection_import(cx))),
                    ),
            );
        }
        if self.connection_bundle.encrypted.is_some() {
            let input = self.connection_bundle.passphrase.clone()?;
            return Some(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_2()
                    .flex_none()
                    .bg(rgb(theme.chrome))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(div().flex_1().child(input))
                    .child(
                        ghost("unlock-connection-import", "Review", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.unlock_connection_import(cx))),
                    )
                    .child(
                        ghost("cancel-connection-import", "Cancel", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_connection_import(cx))),
                    ),
            );
        }
        None
    }
}

fn apply_import(pending: PendingImport) -> Result<usize, String> {
    runtime()?.block_on(async move {
        let database = bookie_app_services::database_service::DatabaseService::new();
        let operation = uuid::Uuid::new_v4();
        let planned = pending.plan.items.len();
        database
            .audit_administrative_action(
                operation,
                bookie_policy::AuditAdministrativeAction::ConnectionBundleImport,
                bookie_policy::AuditRecordPhase::Intent,
                bookie_policy::AuditTerminalStatus::Pending,
                planned,
                None,
            )
            .await
            .map_err(|_| "The import was cancelled because its audit record could not be saved".to_string())?;
        let started = Instant::now();
        let outcome = apply_items(pending).await;
        let (status, affected) = match &outcome {
            Ok(count) => (bookie_policy::AuditTerminalStatus::Succeeded, *count),
            Err((count, _)) => (bookie_policy::AuditTerminalStatus::Failed, *count),
        };
        if database
            .audit_administrative_action(
                operation,
                bookie_policy::AuditAdministrativeAction::ConnectionBundleImport,
                bookie_policy::AuditRecordPhase::Outcome,
                status,
                affected,
                Some(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
            )
            .await
            .is_err()
        {
            return Err("The import finished, but its audit outcome could not be saved".into());
        }
        outcome.map_err(|(count, error)| format!("Imported {count} connection(s); {error}"))
    })
}

async fn apply_items(pending: PendingImport) -> Result<usize, (usize, String)> {
    let mut imported = 0;
    for item in pending.plan.items {
        let carried = pending
            .secrets
            .iter()
            .find(|secret| secret.connection_id == item.source_id);
        let single = bookie_storage::ImportPlan {
            items: vec![item.clone()],
        };
        if let Err(error) = bookie_storage::apply_import(&single).await {
            return Err((imported, format!("the connection could not be saved: {error}")));
        }
        if let Some(secrets) = carried {
            if bookie_storage::store_bundle_secrets(item.connection.id, secrets, &item.connection.name, false)
                .await
                .is_err()
            {
                let restored = bookie_storage::restore_connection(item.previous.as_ref(), item.connection.id)
                    .await
                    .is_ok();
                if restored
                    && matches!(
                        item.disposition,
                        bookie_storage::ImportDisposition::New | bookie_storage::ImportDisposition::Remapped
                    )
                {
                    bookie_storage::forget_imported_bundle_secrets(item.connection.id, secrets).await;
                }
                return Err((imported, "a saved credential could not be stored".into()));
            }
        }
        imported += 1;
    }
    Ok(imported)
}

fn export_connections(path: PathBuf) -> Result<usize, String> {
    runtime()?.block_on(async move {
        let database = bookie_app_services::database_service::DatabaseService::new();
        let operation = uuid::Uuid::new_v4();
        let connections = bookie_storage::load_connections()
            .await
            .map_err(|error| error.to_string())?;
        if connections.is_empty() {
            return Err("There are no saved connections to export".into());
        }
        database
            .audit_administrative_action(
                operation,
                bookie_policy::AuditAdministrativeAction::ConnectionBundleExport,
                bookie_policy::AuditRecordPhase::Intent,
                bookie_policy::AuditTerminalStatus::Pending,
                connections.len(),
                None,
            )
            .await
            .map_err(|_| "The export was cancelled because its audit record could not be saved".to_string())?;
        let started = Instant::now();
        let outcome = write_export(path, connections).await;
        let status = if outcome.is_ok() {
            bookie_policy::AuditTerminalStatus::Succeeded
        } else {
            bookie_policy::AuditTerminalStatus::Failed
        };
        let count = outcome.as_ref().copied().unwrap_or_default();
        if database
            .audit_administrative_action(
                operation,
                bookie_policy::AuditAdministrativeAction::ConnectionBundleExport,
                bookie_policy::AuditRecordPhase::Outcome,
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
    connections: Vec<bookie_storage::SavedConnection>,
) -> Result<usize, bookie_storage::BundleError> {
    let count = connections.len();
    let organization = bookie_storage::load_organization()
        .await
        .map_err(|_| bookie_storage::BundleError::Field("organization"))?;
    let organization = connections
        .iter()
        .map(|connection| (connection.id, organization.get(connection.id)))
        .collect();
    let export = bundle_export(connections, organization);
    let bytes = bookie_storage::export_plaintext(&export)?;
    std::fs::write(path, bytes).map_err(|_| bookie_storage::BundleError::Field("destination"))?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_disposition_text_is_user_visible_and_distinct() {
        assert_ne!(
            disposition_name(&bookie_storage::ImportDisposition::New),
            disposition_name(&bookie_storage::ImportDisposition::UpdateInPlace)
        );
        assert_ne!(
            disposition_name(&bookie_storage::ImportDisposition::Remapped),
            disposition_name(&bookie_storage::ImportDisposition::UpdateInPlace)
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
        let connection: bookie_storage::SavedConnection = serde_json::from_str(
            r#"{"id":"6af4b280-82ec-4d81-b780-3878f7af1647","name":"local","driver_id":"postgres","host":"localhost","port":5432,"database":"test","username":"test"}"#,
        )?;
        let bytes = bookie_storage::export_plaintext(&bundle_export(vec![connection], Vec::new()))?;
        let bookie_storage::ParsedBundle::Plaintext(body) = bookie_storage::parse_bundle(&bytes)? else {
            return Err("plaintext export was unexpectedly encrypted".into());
        };
        assert_eq!(body.connections.len(), 1);
        assert!(body.secrets.is_empty());
        Ok(())
    }
}
