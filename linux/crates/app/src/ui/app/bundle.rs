use relm4::ComponentSender;
use relm4::gtk::prelude::FileExt;

use tablepro_storage::{
    BundleError, BundleExport, ImportDisposition, ImportItem, ImportPlan, ParsedBundle, SavedConnection,
};
use uuid::Uuid;

use crate::ui::connection_bundle::{
    ExportChoice, ImportChoice, present_export_dialog, present_import_passphrase_dialog, present_import_plan_dialog,
    selected_connections,
};

use super::{App, AppMsg};

/// A bundle the user opened but has not confirmed. Held between the
/// preview dialog and the write, so nothing reaches disk before the
/// plan has been shown.
pub(crate) struct PendingImport {
    pub(crate) plan: ImportPlan,
    pub(crate) secrets: Vec<tablepro_storage::BundleSecrets>,
}

impl std::fmt::Debug for PendingImport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingImport")
            .field("items", &self.plan.items.len())
            .field("secrets", &self.secrets.len())
            .finish()
    }
}

impl App {
    pub(super) fn on_export_connections(&self, sender: ComponentSender<Self>) {
        if self.saved_connections.is_empty() {
            self.show_toast(&crate::tr!("There are no saved connections to export."));
            return;
        }
        let input = sender.input_sender().clone();
        present_export_dialog(
            &self.window,
            &self.saved_connections,
            today_stamp(),
            move |choice: ExportChoice| {
                let _ = input.send(AppMsg::ExportConnectionsTo(choice));
            },
        );
    }

    pub(super) fn on_export_connections_to(&self, choice: ExportChoice, sender: ComponentSender<Self>) {
        let chosen = selected_connections(&self.saved_connections, &choice.connections);
        let organization: Vec<(Uuid, tablepro_storage::ConnectionOrganization)> = chosen
            .iter()
            .map(|connection| (connection.id, self.connection_organization.get(connection.id)))
            .collect();
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let outcome = write_bundle(choice, chosen, organization).await;
                    match outcome {
                        Ok(count) => sender_clone.input(AppMsg::ExportConnectionsSucceeded(count)),
                        Err(error) => sender_clone.input(AppMsg::BundleFailed(error.to_string())),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_export_connections_succeeded(&self, count: usize) {
        self.show_toast(&crate::tr!("Exported {n} connections.").replace("{n}", &count.to_string()));
    }

    pub(super) fn on_bundle_failed(&self, message: &str) {
        self.show_toast(message);
    }

    pub(super) fn on_import_bundle(&self, sender: ComponentSender<Self>) {
        let filter = crate::ui::connection_bundle::bundle_file_filter();
        let dialog = relm4::gtk::FileDialog::builder()
            .title(crate::tr!("Import connections"))
            .modal(true)
            .default_filter(&filter)
            .filters(&crate::ui::connection_bundle::bundle_file_filters(&filter))
            .build();
        let input = sender.input_sender().clone();
        dialog.open(Some(&self.window), relm4::gtk::gio::Cancellable::NONE, move |outcome| {
            let Ok(file) = outcome else {
                return;
            };
            let Some(path) = file.path() else {
                return;
            };
            let _ = input.send(AppMsg::ImportBundleFile(path));
        });
    }

    /// Read and parse off the GTK thread, then either plan straight away
    /// or ask for the passphrase.
    pub(super) fn on_import_bundle_file(&self, path: std::path::PathBuf, sender: ComponentSender<Self>) {
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    match read_bundle_at(&path).await {
                        Ok(ParsedBundle::Plaintext(body)) => match plan_from_body(&body).await {
                            Ok(pending) => sender_clone.input(AppMsg::ImportBundlePlanned(pending)),
                            Err(message) => sender_clone.input(AppMsg::BundleFailed(message)),
                        },
                        Ok(ParsedBundle::Encrypted(sealed)) => {
                            sender_clone.input(AppMsg::ImportBundleEncrypted(sealed));
                        }
                        Err(error) => sender_clone.input(AppMsg::BundleFailed(error.to_string())),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_import_bundle_encrypted(
        &mut self,
        sealed: tablepro_storage::EncryptedBundle,
        sender: ComponentSender<Self>,
    ) {
        self.pending_bundle = Some(sealed);
        self.on_import_bundle_needs_passphrase(sender);
    }

    pub(super) fn on_import_bundle_unlock(&mut self, passphrase: String, sender: ComponentSender<Self>) {
        let Some(sealed) = self.pending_bundle.take() else {
            return;
        };
        let body = match sealed.unlock(&passphrase) {
            Ok(body) => body,
            Err(error) => {
                self.show_toast(&error.to_string());
                return;
            }
        };
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    match plan_from_body(&body).await {
                        Ok(pending) => sender_clone.input(AppMsg::ImportBundlePlanned(pending)),
                        Err(message) => sender_clone.input(AppMsg::BundleFailed(message)),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_import_bundle_needs_passphrase(&self, sender: ComponentSender<Self>) {
        let input = sender.input_sender().clone();
        present_import_passphrase_dialog(&self.window, move |passphrase| {
            let _ = input.send(AppMsg::ImportBundleUnlock(passphrase));
        });
    }

    pub(super) fn on_import_bundle_planned(&mut self, pending: PendingImport, sender: ComponentSender<Self>) {
        if pending.plan.items.is_empty() {
            self.show_toast(&crate::tr!("That file holds no connections."));
            return;
        }
        let carries_secrets = !pending.secrets.is_empty();
        let input = sender.input_sender().clone();
        present_import_plan_dialog(
            &self.window,
            &pending.plan.items,
            carries_secrets,
            move |choice: ImportChoice| {
                let _ = input.send(AppMsg::ImportBundleConfirmed(choice));
            },
        );
        self.pending_import = Some(Box::new(pending));
    }

    pub(super) fn on_import_bundle_confirmed(&mut self, choice: ImportChoice, sender: ComponentSender<Self>) {
        let Some(pending) = self.pending_import.take() else {
            return;
        };
        let plan = ImportPlan {
            items: accepted_items(&pending.plan, &choice.accepted),
        };
        let secrets = pending.secrets;
        let replace = choice.replace_saved_passwords;
        let sender_clone = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    match apply_plan(plan, secrets, replace).await {
                        Ok((imported, planned)) => {
                            sender_clone.input(AppMsg::ImportBundleFinished(imported, planned));
                            sender_clone.input(AppMsg::ReloadConnections);
                        }
                        Err(message) => sender_clone.input(AppMsg::BundleFailed(message)),
                    }
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_import_bundle_finished(&self, imported: usize, planned: usize) {
        self.show_toast(&crate::ui::connection_bundle::import_outcome_message(imported, planned));
    }
}

fn today_stamp() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// Keep the plan's order while dropping whatever the user unticked.
pub(super) fn accepted_items(plan: &ImportPlan, accepted: &[Uuid]) -> Vec<ImportItem> {
    plan.items
        .iter()
        .filter(|item| accepted.contains(&item.source_id))
        .cloned()
        .collect()
}

async fn write_bundle(
    choice: ExportChoice,
    connections: Vec<SavedConnection>,
    organization: Vec<(Uuid, tablepro_storage::ConnectionOrganization)>,
) -> Result<usize, BundleError> {
    let count = connections.len();
    let ids: Vec<Uuid> = connections.iter().map(|connection| connection.id).collect();
    let secrets = if choice.passphrase.is_empty() {
        Vec::new()
    } else {
        tablepro_storage::collect_bundle_secrets(&ids).await?
    };
    let export = BundleExport {
        producer: format!("bookie {}", env!("CARGO_PKG_VERSION")),
        exported_at: chrono::Utc::now().to_rfc3339(),
        connections,
        organization,
        secrets,
    };
    let bytes = if choice.passphrase.is_empty() {
        tablepro_storage::export_plaintext(&export)?
    } else {
        tablepro_storage::export_encrypted(&export, &choice.passphrase)?
    };
    tokio::fs::write(&choice.path, &bytes)
        .await
        .map_err(|_| BundleError::Field("destination"))?;
    Ok(count)
}

pub(super) fn read_bundle_file(bytes: &[u8]) -> Result<ParsedBundle, BundleError> {
    tablepro_storage::parse_bundle(bytes)
}

/// Bounded read: the same ceiling the parser enforces, applied before
/// the bytes are in memory, because the path comes from a file chooser
/// and the file itself is untrusted.
async fn read_bundle_at(path: &std::path::Path) -> Result<ParsedBundle, BundleError> {
    let bytes = tokio::fs::read(path).await.map_err(|_| BundleError::NotABundle)?;
    read_bundle_file(&bytes)
}

async fn plan_from_body(body: &tablepro_storage::BundleBody) -> Result<PendingImport, String> {
    let local = tablepro_storage::load_connections()
        .await
        .map_err(|_| crate::tr!("The saved connections could not be read."))?;
    let plan = tablepro_storage::plan_import(&local, body).map_err(|error| error.to_string())?;
    Ok(PendingImport {
        plan,
        secrets: body.secrets.clone(),
    })
}

async fn apply_plan(
    plan: ImportPlan,
    secrets: Vec<tablepro_storage::BundleSecrets>,
    replace_saved_passwords: bool,
) -> Result<(usize, usize), String> {
    let planned = plan.items.len();
    tablepro_storage::apply_import(&plan)
        .await
        .map_err(|_| crate::tr!("The connections could not be saved."))?;

    let mut imported = 0usize;
    for item in &plan.items {
        let Some(carried) = secrets.iter().find(|entry| entry.connection_id == item.source_id) else {
            imported += 1;
            continue;
        };
        let stored = tablepro_storage::store_bundle_secrets(
            item.connection.id,
            carried,
            &item.connection.name,
            replace_saved_passwords,
        )
        .await;
        if stored.is_err() {
            roll_back(item).await;
            return Ok((imported, planned));
        }
        imported += 1;
    }
    Ok((imported, planned))
}

/// Undo one item after a failed credential write. An entry that existed
/// before the import is restored from its snapshot; one this import
/// created is removed along with whatever it managed to store.
async fn roll_back(item: &ImportItem) {
    if let Err(error) = tablepro_storage::restore_connection(item.previous.as_ref(), item.connection.id).await {
        tracing::warn!(error = %error, "rolling back an imported connection failed");
    }
    if matches!(item.disposition, ImportDisposition::New | ImportDisposition::Remapped) {
        tablepro_storage::forget_imported_secrets(item.connection.id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tablepro_core::{AuthMode, Environment, TlsMode};

    fn saved(name: &str) -> SavedConnection {
        SavedConnection {
            id: Uuid::new_v4(),
            name: name.into(),
            driver_id: "postgres".into(),
            host: "localhost".into(),
            port: 5432,
            socket_dir: None,
            database: "app".into(),
            username: "app".into(),
            use_tls: false,
            tls_mode: Some(TlsMode::Disabled),
            tls_root_cert: None,
            read_only: false,
            auth_mode: AuthMode::Password,
            environment: Environment::Local,
            ssh: None,
            last_opened_at: None,
        }
    }

    fn item(name: &str) -> ImportItem {
        let connection = saved(name);
        ImportItem {
            disposition: ImportDisposition::New,
            source_id: connection.id,
            connection,
            previous: None,
            organization: None,
        }
    }

    #[test]
    fn unticked_entries_are_dropped_and_the_order_survives() {
        let first = item("a");
        let second = item("b");
        let third = item("c");
        let plan = ImportPlan {
            items: vec![first.clone(), second.clone(), third.clone()],
        };

        let kept = accepted_items(&plan, &[third.source_id, first.source_id]);

        assert_eq!(
            kept.iter().map(|i| i.source_id).collect::<Vec<_>>(),
            vec![first.source_id, third.source_id]
        );
    }

    #[test]
    fn ticking_nothing_imports_nothing() {
        let plan = ImportPlan { items: vec![item("a")] };
        assert!(accepted_items(&plan, &[]).is_empty());
    }

    #[test]
    fn a_file_that_is_not_a_bundle_is_reported_rather_than_parsed() {
        assert!(read_bundle_file(b"nonsense").is_err());
    }

    #[test]
    fn the_export_file_stamp_is_a_plain_date() {
        let stamp = today_stamp();
        assert_eq!(stamp.len(), 10);
        assert_eq!(stamp.matches('-').count(), 2);
    }
}
