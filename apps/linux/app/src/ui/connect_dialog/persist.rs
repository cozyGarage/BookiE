use bookie_app_services::connection_save::HeadSecret;
use bookie_storage::SavedConnection;
use secrecy::{ExposeSecret, SecretString};

use super::connect_failure::ConnectFailure;
use crate::ui::ssh_section::{SshInputs, SshSecretToStore};

pub(super) struct PersistRequest<'a> {
    pub saved: &'a SavedConnection,
    pub existing: Option<&'a SavedConnection>,
    pub stored_password: &'a SecretString,
    pub label: &'a str,
    pub ssh: Option<&'a SshInputs>,
}

fn head_secret(inputs: &SshInputs) -> HeadSecret {
    let copy = |secret: &SecretString| SecretString::new(secret.expose_secret().to_owned().into());
    match &inputs.secret_to_store {
        SshSecretToStore::Password(password) => HeadSecret::Password(copy(password)),
        SshSecretToStore::Passphrase(passphrase) => HeadSecret::Passphrase(copy(passphrase)),
        SshSecretToStore::None => HeadSecret::None,
    }
}

pub(super) async fn persist_connection(request: PersistRequest<'_>) -> Result<(), ConnectFailure> {
    let secret = request.ssh.map(head_secret);
    let ssh = request
        .ssh
        .zip(secret.as_ref())
        .map(|(inputs, secret)| (&inputs.chain, secret));
    bookie_app_services::connection_save::persist_connection(
        request.saved,
        request.existing,
        Some(request.stored_password),
        request.label,
        ssh,
    )
    .await
    .map_err(ConnectFailure::from)
}
