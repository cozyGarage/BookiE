use bookie_ssh::{SshAuth, SshConfig};
use secrecy::SecretString;
use thiserror::Error;
use uuid::Uuid;

use crate::ssh_chain_form::{ChainPlan, JumpConnect, JumpConnectAuth, SecretSource};
use crate::ssh_chain_secrets::{HopSecretError, HopSecretStore, SecretServiceStore};

#[derive(Debug, Error)]
pub enum ChainConnectError {
    #[error("the saved secret for jump host {position} is not in the keyring; enter it again")]
    SecretMissing { position: usize },
    #[error(transparent)]
    Secrets(#[from] HopSecretError),
}

pub async fn resolve_jump_hops(
    connection_id: Option<Uuid>,
    plan: &ChainPlan,
) -> Result<Vec<SshConfig>, ChainConnectError> {
    resolve_with(&SecretServiceStore, connection_id, plan).await
}

pub(crate) async fn resolve_with<S: HopSecretStore>(
    store: &S,
    connection_id: Option<Uuid>,
    plan: &ChainPlan,
) -> Result<Vec<SshConfig>, ChainConnectError> {
    let mut hops = Vec::with_capacity(plan.jumps.len());
    for (index, jump) in plan.jumps.iter().enumerate() {
        hops.push(resolve_hop(store, connection_id, jump, index + 2).await?);
    }
    Ok(hops)
}

async fn resolve_hop<S: HopSecretStore>(
    store: &S,
    connection_id: Option<Uuid>,
    jump: &JumpConnect,
    position: usize,
) -> Result<SshConfig, ChainConnectError> {
    let auth = match &jump.auth {
        JumpConnectAuth::Password(source) => SshAuth::Password {
            password: secret(store, connection_id, source, position).await?,
        },
        JumpConnectAuth::PrivateKey { path, passphrase } => SshAuth::PrivateKey {
            path: path.clone(),
            passphrase: match passphrase {
                Some(source) => Some(secret(store, connection_id, source, position).await?),
                None => None,
            },
        },
        JumpConnectAuth::Agent(None) => SshAuth::Agent,
        JumpConnectAuth::Agent(Some(public_key)) => SshAuth::AgentIdentity {
            public_key: public_key.clone(),
        },
    };
    Ok(SshConfig {
        host: jump.host.clone(),
        port: jump.port,
        username: jump.username.clone(),
        auth,
        local_port: None,
    })
}

async fn secret<S: HopSecretStore>(
    store: &S,
    connection_id: Option<Uuid>,
    source: &SecretSource,
    position: usize,
) -> Result<SecretString, ChainConnectError> {
    match source {
        SecretSource::Entered(value) => Ok(value.clone()),
        SecretSource::Stored(reference) => {
            let connection_id = connection_id.ok_or(ChainConnectError::SecretMissing { position })?;
            store
                .load(connection_id, *reference)
                .await
                .map_err(HopSecretError::from)?
                .ok_or(ChainConnectError::SecretMissing { position })
        }
    }
}

#[cfg(test)]
#[path = "ssh_chain_connect_tests.rs"]
mod tests;
