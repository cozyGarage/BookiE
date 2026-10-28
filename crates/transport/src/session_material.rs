use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use tablepro_storage::{SavedConnection, SavedSshAuth, load_password};

use crate::{TransportError, jump_hop_password_refused, loads_database_password};

const MAX_MATERIAL_FILE_BYTES: u64 = 1024 * 1024;

pub async fn session_material_digest(saved: &SavedConnection) -> Result<[u8; 32], TransportError> {
    let mut hasher = MaterialHasher::new();
    append_db_password(&mut hasher, saved).await?;
    append_ssh_material(&mut hasher, saved).await?;
    append_tls_ca(&mut hasher, saved)?;
    append_tls_client_identity(&mut hasher, saved)?;
    Ok(hasher.finish())
}

struct MaterialHasher(Sha256);

impl MaterialHasher {
    fn new() -> Self {
        Self(Sha256::new())
    }

    fn tag(&mut self, tag: &[u8], bytes: &[u8]) {
        self.0.update((tag.len() as u64).to_le_bytes());
        self.0.update(tag);
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

async fn append_db_password(hasher: &mut MaterialHasher, saved: &SavedConnection) -> Result<(), TransportError> {
    let password = if loads_database_password(saved) {
        load_password(saved.id)
            .await
            .map_err(|error| crate::secret_error("load database password", error))?
            .unwrap_or_else(|| SecretString::new(String::new().into()))
    } else {
        SecretString::new(String::new().into())
    };
    hasher.tag(b"db_password", password.expose_secret().as_bytes());
    Ok(())
}

async fn append_ssh_material(hasher: &mut MaterialHasher, saved: &SavedConnection) -> Result<(), TransportError> {
    let Some(ssh) = &saved.ssh else {
        return Ok(());
    };
    for (index, hop) in ssh.flatten_hops().into_iter().enumerate() {
        hasher.tag(b"ssh_hop_id", hop.hop_id.as_bytes());
        hasher.tag(b"ssh_credential_revision", &hop.credential_revision.to_le_bytes());
        if hop.agent {
            hasher.tag(b"ssh_auth", b"agent");
            continue;
        }
        match &hop.auth {
            SavedSshAuth::Password => {
                if hop.credential_revision == 0 && index > 0 {
                    return Err(jump_hop_password_refused(index));
                }
                let password = crate::saved_ssh_password(saved.id, hop, index).await?;
                hasher.tag(b"ssh_password", password.expose_secret().as_bytes());
            }
            SavedSshAuth::PrivateKey { path, has_passphrase } => {
                hasher.tag(b"ssh_key", &read_material_file(path)?);
                if *has_passphrase {
                    match crate::saved_ssh_passphrase(saved.id, hop, index, *has_passphrase).await? {
                        Some(passphrase) => hasher.tag(b"ssh_passphrase", passphrase.expose_secret().as_bytes()),
                        None => hasher.tag(b"ssh_passphrase", &[]),
                    }
                }
            }
        }
    }
    Ok(())
}

fn append_tls_ca(hasher: &mut MaterialHasher, saved: &SavedConnection) -> Result<(), TransportError> {
    let Some(path) = saved.tls_root_cert.as_deref() else {
        return Ok(());
    };
    hasher.tag(b"tls_ca", &read_material_file(path)?);
    Ok(())
}

fn append_tls_client_identity(hasher: &mut MaterialHasher, saved: &SavedConnection) -> Result<(), TransportError> {
    match (saved.tls_client_cert.as_deref(), saved.tls_client_key.as_deref()) {
        (None, None) => Ok(()),
        (Some(cert), Some(key)) => {
            hasher.tag(b"tls_client_cert", &read_material_file(cert)?);
            hasher.tag(b"tls_client_key", &read_material_file(key)?);
            Ok(())
        }
        _ => Err(TransportError::Driver(tablepro_core::DriverError::Unsupported(
            "TLS client authentication requires both a certificate and a private key".into(),
        ))),
    }
}

fn read_material_file(path: &Path) -> Result<Vec<u8>, TransportError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| TransportError::Secret(format!("cannot read {}: {error}", path.display())))?;
    let metadata = file
        .metadata()
        .map_err(|error| TransportError::Secret(format!("cannot inspect {}: {error}", path.display())))?;
    if !metadata.is_file() {
        return Err(TransportError::Secret(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    if metadata.len() > MAX_MATERIAL_FILE_BYTES {
        return Err(TransportError::Secret(format!(
            "{} exceeds the {}-byte material limit",
            path.display(),
            MAX_MATERIAL_FILE_BYTES
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_MATERIAL_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| TransportError::Secret(format!("cannot read {}: {error}", path.display())))?;
    if bytes.len() as u64 > MAX_MATERIAL_FILE_BYTES {
        return Err(TransportError::Secret(format!(
            "{} exceeds the {}-byte material limit",
            path.display(),
            MAX_MATERIAL_FILE_BYTES
        )));
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "session_material_tests.rs"]
mod tests;
