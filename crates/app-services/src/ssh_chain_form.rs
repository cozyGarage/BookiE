use std::collections::HashMap;
use std::path::PathBuf;

use bookie_storage::{BUNDLE_MAX_SSH_HOPS, SavedSshAuth, SavedSshConfig, SshClient};
use secrecy::SecretString;
use thiserror::Error;
use uuid::Uuid;

pub const MAX_CHAIN_HOPS: usize = BUNDLE_MAX_SSH_HOPS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpAuth {
    Password,
    PrivateKey,
    Agent,
}

#[derive(Debug, Clone)]
pub struct JumpHopForm {
    pub hop_id: Option<Uuid>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: JumpAuth,
    pub key_path: String,
    pub agent_identity: String,
    pub password: Option<SecretString>,
    pub passphrase: Option<SecretString>,
    pub stored_password: bool,
    pub stored_passphrase: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ChainFormError {
    #[error("SSH hop {position} needs a host")]
    EmptyHost { position: usize },
    #[error("SSH hop {position} needs a port between 1 and 65535")]
    InvalidPort { position: usize },
    #[error("SSH hop {position} needs a username")]
    EmptyUsername { position: usize },
    #[error("SSH hop {position} needs a private key path")]
    EmptyKeyPath { position: usize },
    #[error("SSH hop {position} needs a password")]
    PasswordRequired { position: usize },
    #[error("SSH hop {position} repeats a host already in the chain")]
    RepeatedHop { position: usize },
    #[error("an SSH chain is limited to {limit} hops")]
    TooManyHops { limit: usize },
    #[error("the system OpenSSH client cannot use saved jump hosts")]
    OpenSshRefusesJumpHops,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HopSecretKind {
    Password,
    Passphrase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HopSecretRef {
    pub hop_id: Uuid,
    pub revision: u64,
    pub kind: HopSecretKind,
}

#[derive(Debug, Clone)]
pub struct HopSecretWrite {
    pub secret: HopSecretRef,
    pub value: SecretString,
}

#[derive(Debug, Clone)]
pub enum SecretSource {
    Entered(SecretString),
    Stored(HopSecretRef),
}

#[derive(Debug, Clone)]
pub enum JumpConnectAuth {
    Password(SecretSource),
    PrivateKey {
        path: PathBuf,
        passphrase: Option<SecretSource>,
    },
    Agent(Option<PathBuf>),
}

#[derive(Debug, Clone)]
pub struct JumpConnect {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: JumpConnectAuth,
}

#[derive(Debug, Clone)]
pub struct ChainPlan {
    pub head: SavedSshConfig,
    pub jumps: Vec<JumpConnect>,
    pub writes: Vec<HopSecretWrite>,
    pub obsolete: Vec<HopSecretRef>,
}

pub fn plan_chain(
    head: SavedSshConfig,
    forms: &[JumpHopForm],
    previous: Option<&SavedSshConfig>,
) -> Result<ChainPlan, ChainFormError> {
    if forms.is_empty() {
        let mut obsolete = removed_hop_secrets(previous, &[head.hop_id]);
        obsolete.extend(replaced_head_secrets(previous, &head));
        return Ok(ChainPlan {
            obsolete,
            head: SavedSshConfig { jump: None, ..head },
            jumps: Vec::new(),
            writes: Vec::new(),
        });
    }
    validate_chain(&head, forms)?;
    let earlier: HashMap<Uuid, &SavedSshConfig> = previous
        .map(|chain| chain.flatten_hops().into_iter().map(|hop| (hop.hop_id, hop)).collect())
        .unwrap_or_default();
    let mut writes = Vec::new();
    let mut obsolete = Vec::new();
    let mut jumps = Vec::with_capacity(forms.len());
    let mut saved = Vec::with_capacity(forms.len());
    for form in forms {
        let hop_id = form.hop_id.unwrap_or_else(Uuid::new_v4);
        let planned = plan_hop(form, hop_id, form.hop_id.and_then(|id| earlier.get(&id).copied()));
        writes.extend(planned.writes);
        obsolete.extend(planned.obsolete);
        jumps.push(planned.connect);
        saved.push(planned.saved);
    }
    let mut kept: Vec<Uuid> = saved.iter().map(|hop| hop.hop_id).collect();
    kept.push(head.hop_id);
    obsolete.extend(removed_hop_secrets(previous, &kept));
    obsolete.extend(replaced_head_secrets(previous, &head));
    Ok(ChainPlan {
        head: nest_hops(head, saved),
        jumps,
        writes,
        obsolete,
    })
}

struct PlannedHop {
    saved: SavedSshConfig,
    connect: JumpConnect,
    writes: Vec<HopSecretWrite>,
    obsolete: Vec<HopSecretRef>,
}

fn plan_hop(form: &JumpHopForm, hop_id: Uuid, earlier: Option<&SavedSshConfig>) -> PlannedHop {
    let old_revision = earlier.map_or(0, |hop| hop.credential_revision);
    let changed = match form.auth {
        JumpAuth::Password => form.password.is_some(),
        JumpAuth::PrivateKey => form.passphrase.is_some(),
        JumpAuth::Agent => false,
    };
    let revision = if changed { old_revision + 1 } else { old_revision };
    let keeps_passphrase = form.auth == JumpAuth::PrivateKey && (form.passphrase.is_some() || form.stored_passphrase);
    let auth = saved_auth(form, keeps_passphrase);
    let connect = connect_auth(form, hop_id, revision, keeps_passphrase);
    let writes = secret_writes(form, hop_id, revision);
    let obsolete = earlier.map_or_else(Vec::new, |hop| obsolete_secrets(hop, form, revision, keeps_passphrase));
    let agent = form.auth == JumpAuth::Agent;
    let identity = form.agent_identity.trim();
    PlannedHop {
        saved: SavedSshConfig {
            hop_id,
            credential_revision: revision,
            host: form.host.trim().to_owned(),
            port: form.port,
            username: form.username.trim().to_owned(),
            auth,
            jump: None,
            client: SshClient::Builtin,
            agent,
            agent_identity: (agent && !identity.is_empty()).then(|| PathBuf::from(identity)),
            local_port: None,
        },
        connect: JumpConnect {
            host: form.host.trim().to_owned(),
            port: form.port,
            username: form.username.trim().to_owned(),
            auth: connect,
        },
        writes,
        obsolete,
    }
}

fn saved_auth(form: &JumpHopForm, has_passphrase: bool) -> SavedSshAuth {
    match form.auth {
        JumpAuth::Password => SavedSshAuth::Password,
        JumpAuth::PrivateKey => SavedSshAuth::PrivateKey {
            path: PathBuf::from(form.key_path.trim()),
            has_passphrase,
        },
        JumpAuth::Agent => SavedSshAuth::PrivateKey {
            path: PathBuf::from(form.key_path.trim()),
            has_passphrase: false,
        },
    }
}

fn connect_auth(form: &JumpHopForm, hop_id: Uuid, revision: u64, has_passphrase: bool) -> JumpConnectAuth {
    let stored = |kind| SecretSource::Stored(HopSecretRef { hop_id, revision, kind });
    match form.auth {
        JumpAuth::Password => JumpConnectAuth::Password(
            form.password
                .clone()
                .map_or_else(|| stored(HopSecretKind::Password), SecretSource::Entered),
        ),
        JumpAuth::PrivateKey => JumpConnectAuth::PrivateKey {
            path: PathBuf::from(form.key_path.trim()),
            passphrase: has_passphrase.then(|| {
                form.passphrase
                    .clone()
                    .map_or_else(|| stored(HopSecretKind::Passphrase), SecretSource::Entered)
            }),
        },
        JumpAuth::Agent => {
            let identity = form.agent_identity.trim();
            JumpConnectAuth::Agent((!identity.is_empty()).then(|| PathBuf::from(identity)))
        }
    }
}

fn secret_writes(form: &JumpHopForm, hop_id: Uuid, revision: u64) -> Vec<HopSecretWrite> {
    let write = |kind, value: &SecretString| HopSecretWrite {
        secret: HopSecretRef { hop_id, revision, kind },
        value: value.clone(),
    };
    match (form.auth, &form.password, &form.passphrase) {
        (JumpAuth::Password, Some(password), _) => vec![write(HopSecretKind::Password, password)],
        (JumpAuth::PrivateKey, _, Some(passphrase)) => vec![write(HopSecretKind::Passphrase, passphrase)],
        _ => Vec::new(),
    }
}

fn obsolete_secrets(
    earlier: &SavedSshConfig,
    form: &JumpHopForm,
    revision: u64,
    keeps_passphrase: bool,
) -> Vec<HopSecretRef> {
    if earlier.credential_revision == 0 {
        return Vec::new();
    }
    let stale = |kind| HopSecretRef {
        hop_id: earlier.hop_id,
        revision: earlier.credential_revision,
        kind,
    };
    let rotated = revision != earlier.credential_revision;
    let had_password = matches!(earlier.auth, SavedSshAuth::Password);
    let had_passphrase = matches!(
        earlier.auth,
        SavedSshAuth::PrivateKey {
            has_passphrase: true,
            ..
        }
    );
    let mut out = Vec::new();
    if had_password && (rotated || form.auth != JumpAuth::Password) {
        out.push(stale(HopSecretKind::Password));
    }
    if had_passphrase && (rotated || !keeps_passphrase) {
        out.push(stale(HopSecretKind::Passphrase));
    }
    out
}

fn removed_hop_secrets(previous: Option<&SavedSshConfig>, kept: &[Uuid]) -> Vec<HopSecretRef> {
    let Some(previous) = previous else {
        return Vec::new();
    };
    previous
        .flatten_hops()
        .into_iter()
        .filter(|hop| hop.credential_revision > 0 && !kept.contains(&hop.hop_id))
        .flat_map(|hop| {
            [HopSecretKind::Password, HopSecretKind::Passphrase].map(|kind| HopSecretRef {
                hop_id: hop.hop_id,
                revision: hop.credential_revision,
                kind,
            })
        })
        .collect()
}

fn replaced_head_secrets(previous: Option<&SavedSshConfig>, head: &SavedSshConfig) -> Vec<HopSecretRef> {
    let Some(previous) = previous else {
        return Vec::new();
    };
    if previous.hop_id != head.hop_id || previous.credential_revision == 0 || head.credential_revision != 0 {
        return Vec::new();
    }
    [HopSecretKind::Password, HopSecretKind::Passphrase]
        .map(|kind| HopSecretRef {
            hop_id: previous.hop_id,
            revision: previous.credential_revision,
            kind,
        })
        .to_vec()
}

fn nest_hops(head: SavedSshConfig, hops: Vec<SavedSshConfig>) -> SavedSshConfig {
    let tail = hops
        .into_iter()
        .rev()
        .fold(None, |next: Option<Box<SavedSshConfig>>, hop| {
            Some(Box::new(SavedSshConfig { jump: next, ..hop }))
        });
    SavedSshConfig { jump: tail, ..head }
}

fn validate_chain(head: &SavedSshConfig, forms: &[JumpHopForm]) -> Result<(), ChainFormError> {
    if forms.len() + 1 > MAX_CHAIN_HOPS {
        return Err(ChainFormError::TooManyHops { limit: MAX_CHAIN_HOPS });
    }
    if head.client == SshClient::OpenSsh {
        return Err(ChainFormError::OpenSshRefusesJumpHops);
    }
    let mut seen = vec![(head.host.trim().to_lowercase(), head.port)];
    for (index, form) in forms.iter().enumerate() {
        let position = index + 2;
        validate_hop(form, position)?;
        let endpoint = (form.host.trim().to_lowercase(), form.port);
        if seen.contains(&endpoint) {
            return Err(ChainFormError::RepeatedHop { position });
        }
        seen.push(endpoint);
    }
    Ok(())
}

fn validate_hop(form: &JumpHopForm, position: usize) -> Result<(), ChainFormError> {
    if form.host.trim().is_empty() {
        return Err(ChainFormError::EmptyHost { position });
    }
    if form.port == 0 {
        return Err(ChainFormError::InvalidPort { position });
    }
    if form.username.trim().is_empty() {
        return Err(ChainFormError::EmptyUsername { position });
    }
    match form.auth {
        JumpAuth::PrivateKey if form.key_path.trim().is_empty() => Err(ChainFormError::EmptyKeyPath { position }),
        JumpAuth::Password if form.password.is_none() && !form.stored_password => {
            Err(ChainFormError::PasswordRequired { position })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "ssh_chain_form_tests.rs"]
mod tests;
