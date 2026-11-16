use relm4::adw;
use relm4::adw::prelude::*;
use secrecy::{ExposeSecret, SecretString};

use bookie_app_services::ssh_chain_form::{HopSecretKind, HopSecretRef};
use bookie_app_services::ssh_chain_secrets::hop_secret_is_stored;
use bookie_core::{AuthMode, Environment};
use bookie_storage::{
    SavedConnection, SavedSshAuth, SavedSshConfig, StorageError, load_password, load_ssh_hop_passphrase,
    load_ssh_hop_password, load_ssh_passphrase, load_ssh_password,
};

use super::{AUTH_MODE_ROWS, ConnectDialog, DriverEntry};
use crate::ui::ssh_jumps::JumpPrefill;

#[derive(Debug)]
pub struct ConnectionPrefill {
    pub saved: SavedConnection,
    pub password: Option<SecretString>,
    pub ssh_password: Option<SecretString>,
    pub ssh_passphrase: Option<SecretString>,
    pub jump_hops: Vec<JumpPrefill>,
    pub unreadable_secrets: bool,
    pub prompt_for_password: bool,
}

pub fn readable(result: Result<Option<SecretString>, StorageError>, unreadable: &mut bool) -> Option<SecretString> {
    match result {
        Ok(secret) => secret,
        Err(error) => {
            tracing::warn!(%error, "a saved secret could not be read for editing");
            *unreadable = true;
            None
        }
    }
}

pub async fn load_prefill(saved: SavedConnection) -> ConnectionPrefill {
    let mut unreadable_secrets = false;
    let password = match saved.auth_mode {
        AuthMode::Password => readable(load_password(saved.id).await, &mut unreadable_secrets),
        AuthMode::Kerberos => None,
    };
    let (ssh_password, ssh_passphrase) = match &saved.ssh {
        Some(head) => head_secrets(saved.id, head, &mut unreadable_secrets).await,
        None => (None, None),
    };
    let jump_hops = load_jump_hops(&saved, &mut unreadable_secrets).await;
    ConnectionPrefill {
        saved,
        password,
        ssh_password,
        ssh_passphrase,
        jump_hops,
        unreadable_secrets,
        prompt_for_password: false,
    }
}

async fn head_secrets(
    id: uuid::Uuid,
    head: &SavedSshConfig,
    unreadable: &mut bool,
) -> (Option<SecretString>, Option<SecretString>) {
    let hop_level = head.credential_revision > 0;
    match (&head.auth, head.agent) {
        (SavedSshAuth::Password, false) if hop_level => (
            readable(
                load_ssh_hop_password(id, head.hop_id, head.credential_revision).await,
                unreadable,
            ),
            None,
        ),
        (SavedSshAuth::Password, false) => (readable(load_ssh_password(id).await, unreadable), None),
        (
            SavedSshAuth::PrivateKey {
                has_passphrase: true, ..
            },
            false,
        ) if hop_level => (
            None,
            readable(
                load_ssh_hop_passphrase(id, head.hop_id, head.credential_revision).await,
                unreadable,
            ),
        ),
        (
            SavedSshAuth::PrivateKey {
                has_passphrase: true, ..
            },
            false,
        ) => (None, readable(load_ssh_passphrase(id).await, unreadable)),
        _ => (None, None),
    }
}

async fn load_jump_hops(saved: &SavedConnection, unreadable: &mut bool) -> Vec<JumpPrefill> {
    let Some(head) = &saved.ssh else {
        return Vec::new();
    };
    let mut hops = Vec::new();
    for hop in head.flatten_hops().into_iter().skip(1) {
        let stored = |kind| HopSecretRef {
            hop_id: hop.hop_id,
            revision: hop.credential_revision,
            kind,
        };
        let present = |result: Result<bool, bookie_app_services::ssh_chain_secrets::HopSecretError>,
                       flag: &mut bool| {
            result.unwrap_or_else(|error| {
                tracing::warn!(%error, "a saved jump host secret could not be checked");
                *flag = true;
                false
            })
        };
        let has_password = hop.credential_revision > 0
            && matches!(hop.auth, SavedSshAuth::Password)
            && present(
                hop_secret_is_stored(saved.id, stored(HopSecretKind::Password)).await,
                unreadable,
            );
        let has_passphrase = hop.credential_revision > 0
            && matches!(
                hop.auth,
                SavedSshAuth::PrivateKey {
                    has_passphrase: true,
                    ..
                }
            )
            && present(
                hop_secret_is_stored(saved.id, stored(HopSecretKind::Passphrase)).await,
                unreadable,
            );
        hops.push(JumpPrefill {
            saved: SavedSshConfig {
                jump: None,
                ..hop.clone()
            },
            has_password,
            has_passphrase,
        });
    }
    hops
}

pub(super) fn environment_row(environment: Environment) -> u32 {
    match environment {
        Environment::Local => 0,
        Environment::Dev => 1,
        Environment::Staging => 2,
        Environment::Prod => 3,
    }
}

pub(super) fn auth_row(mode: AuthMode) -> u32 {
    AUTH_MODE_ROWS.iter().position(|row| *row == mode).unwrap_or(0) as u32
}

pub(super) fn driver_row(drivers: &[DriverEntry], driver_id: &str) -> Option<u32> {
    drivers.iter().position(|d| d.id == driver_id).map(|row| row as u32)
}

fn seconds_row(seconds: Option<u32>) -> f64 {
    f64::from(seconds.unwrap_or(0))
}

impl ConnectDialog {
    fn prefill_tls(&self, saved: &SavedConnection) {
        self.tls_mode.set_selected(saved.effective_tls_mode().index());
        if let Some(root_cert) = &saved.tls_root_cert {
            self.tls_root_cert.set_text(&root_cert.to_string_lossy());
        }
        if let Some(client_cert) = &saved.tls_client_cert {
            self.tls_client_cert.set_text(&client_cert.to_string_lossy());
        }
        if let Some(client_key) = &saved.tls_client_key {
            self.tls_client_key.set_text(&client_key.to_string_lossy());
        }
        self.tls_strict.set_active(saved.tls_strict_encryption);
        self.apply_tls_visibility();
    }

    pub(super) fn apply_prefill(&mut self, prefill: &ConnectionPrefill, root: &adw::Dialog) {
        let saved = &prefill.saved;
        if let Some(row) = driver_row(&self.drivers, &saved.driver_id) {
            self.driver_combo.set_selected(row);
            self.apply_selected_driver(row, root);
        }
        self.name.set_text(&saved.name);
        self.host.set_text(&saved.host);
        self.port.set_value(f64::from(saved.port));
        self.database.set_text(&saved.database);
        super::auth_source::set(&self.auth_source, saved);
        self.username.set_text(&saved.username);
        if let Some(dir) = &saved.socket_dir {
            self.endpoint_combo.set_selected(1);
            self.socket_dir.set_text(&dir.to_string_lossy());
        }
        if let Some(path) = &saved.socket_path {
            self.endpoint_combo.set_selected(1);
            self.socket_dir.set_text(&path.to_string_lossy());
        }
        self.auth_combo.set_selected(auth_row(saved.auth_mode));
        self.form.selected = saved.auth_mode;
        self.apply_form_state();
        self.prefill_tls(saved);
        self.read_only.set_active(saved.read_only);
        self.environment.set_selected(environment_row(saved.environment));
        self.connect_timeout.set_value(seconds_row(saved.connect_timeout_secs));
        self.query_timeout.set_value(seconds_row(saved.query_timeout_secs));
        if let Some(password) = &prefill.password {
            self.password.set_text(password.expose_secret());
        }
        if let Some(ssh) = &saved.ssh {
            self.ssh
                .prefill(ssh, prefill.ssh_password.as_ref(), prefill.ssh_passphrase.as_ref());
            self.ssh.jumps.prefill(ssh, &prefill.jump_hops);
        }
        if prefill.prompt_for_password {
            self.password.grab_focus();
        }
        let title = if prefill.prompt_for_password {
            bookie_app_services::tr!("Enter password for {name}").replace("{name}", &saved.name)
        } else {
            bookie_app_services::tr!("Edit {name}").replace("{name}", &saved.name)
        };
        root.set_title(&title);
        self.header_title.set_text(&title);
        if prefill.unreadable_secrets {
            self.show_toast(&bookie_app_services::tr!(
                "The saved password could not be read from the keyring. Enter it again to keep password sign-in."
            ));
        }
        self.refresh_validity();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::DriverMaturity;
    use bookie_storage::KeyringFailure;

    fn entry(id: &str) -> DriverEntry {
        DriverEntry {
            id: id.to_string(),
            display_name: id.to_string(),
            maturity: DriverMaturity::Stable,
        }
    }

    #[test]
    fn environment_rows_match_the_form_order() {
        let rows: Vec<u32> = [
            Environment::Local,
            Environment::Dev,
            Environment::Staging,
            Environment::Prod,
        ]
        .into_iter()
        .map(environment_row)
        .collect();
        assert_eq!(rows, vec![0, 1, 2, 3]);
    }

    #[test]
    fn auth_rows_round_trip_through_the_form_decoder() {
        for mode in AUTH_MODE_ROWS {
            assert_eq!(super::super::auth_mode_for_row(auth_row(mode)), mode);
        }
    }

    #[test]
    fn driver_row_finds_the_sorted_position_or_nothing() {
        let drivers = [entry("mysql"), entry("postgres"), entry("sqlite")];
        assert_eq!(driver_row(&drivers, "postgres"), Some(1));
        assert_eq!(driver_row(&drivers, "oracle"), None);
    }

    #[test]
    fn a_secret_that_cannot_be_read_is_flagged_instead_of_failing_the_edit() {
        let mut unreadable = false;
        let kept = readable(Ok(Some(SecretString::new("pw".into()))), &mut unreadable);
        assert_eq!(kept.map(|s| s.expose_secret().to_string()), Some("pw".to_string()));
        assert!(!unreadable);

        assert!(readable(Ok(None), &mut unreadable).is_none());
        assert!(!unreadable);

        assert!(readable(Err(StorageError::Keyring(KeyringFailure::Unavailable)), &mut unreadable).is_none());
        assert!(unreadable);
    }

    #[test]
    fn unset_timeouts_show_zero() {
        assert_eq!(seconds_row(None), 0.0);
        assert_eq!(seconds_row(Some(45)), 45.0);
    }
}
