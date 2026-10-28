use tablepro_core::ConnectOptions;
use tablepro_storage::{SavedConnection, SavedSshConfig};
use uuid::Uuid;

use crate::ui::ssh_section::SshInputs;

/// Which saved record the dialog is about to write. `bound_id` is set
/// when the dialog was opened against a known connection; it decides on
/// its own and endpoint matching is not consulted.
pub(super) struct ConnectionIdentity<'a> {
    pub bound_id: Option<Uuid>,
    pub driver_id: &'a str,
    pub opts: &'a ConnectOptions,
    pub file_based: bool,
    pub ssh: Option<&'a SshInputs>,
}

/// The name a saved record gets. A name the user typed wins; blank
/// falls back to the endpoint-derived label, which is what every
/// connection saved before naming shipped already carries.
pub(super) fn saved_connection_name(entered: &str, driver_id: &str, opts: &ConnectOptions) -> String {
    let entered = entered.trim();
    if !entered.is_empty() {
        return entered.to_owned();
    }
    derived_connection_name(driver_id, opts)
}

fn derived_connection_name(driver_id: &str, opts: &ConnectOptions) -> String {
    if driver_id == "sqlite" {
        return opts.database.clone();
    }
    if let Some(directory) = &opts.local_socket_dir {
        return format!("{}@{}", opts.username, directory.display());
    }
    if let Some(path) = &opts.local_socket_path {
        return format!("{}@{}", opts.username, path.display());
    }
    if opts.auth_mode == tablepro_core::AuthMode::Kerberos {
        return opts.host.clone();
    }
    format!("{}@{}", opts.username, opts.host)
}

pub(super) async fn find_existing(identity: &ConnectionIdentity<'_>) -> Option<SavedConnection> {
    let existing = tablepro_storage::load_connections().await.ok()?;
    select_existing(&existing, identity).cloned()
}

pub(super) fn select_existing<'a>(
    existing: &'a [SavedConnection],
    identity: &ConnectionIdentity<'_>,
) -> Option<&'a SavedConnection> {
    if let Some(id) = identity.bound_id {
        return existing.iter().find(|saved| saved.id == id);
    }
    let candidates: Vec<_> = existing
        .iter()
        .filter(|saved| matches_endpoint(saved, identity))
        .collect();
    candidates
        .iter()
        .copied()
        .find(|saved| {
            identity.file_based
                || saved.username == identity.opts.username && saved.auth_mode == identity.opts.auth_mode
        })
        .or_else(|| (candidates.len() == 1).then(|| candidates[0]))
}

fn matches_endpoint(saved: &SavedConnection, identity: &ConnectionIdentity<'_>) -> bool {
    let opts = identity.opts;
    if saved.driver_id != identity.driver_id || saved.database != opts.database || saved.auth_source != opts.auth_source
    {
        return false;
    }
    if saved.tls_client_cert != opts.tls.client_cert || saved.tls_client_key != opts.tls.client_key {
        return false;
    }
    if identity.file_based {
        return true;
    }
    match (
        &saved.socket_dir,
        &saved.socket_path,
        &opts.local_socket_dir,
        &opts.local_socket_path,
    ) {
        (Some(saved_dir), None, Some(current_dir), None) => {
            saved_dir == current_dir && saved.port == opts.port && saved.ssh.is_none()
        }
        (None, Some(saved_path), None, Some(current_path)) => saved_path == current_path && saved.ssh.is_none(),
        (None, None, None, None) => {
            saved.host == opts.host && saved.port == opts.port && saved_ssh_matches(&saved.ssh, identity.ssh)
        }
        _ => false,
    }
}

fn saved_ssh_matches(saved: &Option<SavedSshConfig>, current: Option<&SshInputs>) -> bool {
    match (saved, current) {
        (None, None) => true,
        (Some(s), Some(c)) => &c.saved == s,
        _ => false,
    }
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
