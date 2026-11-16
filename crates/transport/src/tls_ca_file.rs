use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use bookie_core::{TlsConfig, TlsMode};
use rustls::RootCertStore;
use rustls_pki_types::{CertificateDer, pem::PemObject};

const MAX_CA_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum TlsCaFileError {
    #[error("cannot read TLS certificate authority file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("TLS certificate authority file {path} is not a regular file")]
    NotRegular { path: PathBuf },
    #[error("TLS certificate authority file {path} exceeds the {MAX_CA_FILE_BYTES}-byte limit")]
    TooLarge { path: PathBuf },
    #[error("cannot parse a certificate in TLS certificate authority file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: rustls_pki_types::pem::Error,
    },
    #[error("TLS certificate authority file {path} contains no certificates")]
    Empty { path: PathBuf },
    #[error("TLS certificate authority file {path} contains an invalid certificate: {source}")]
    InvalidCertificate {
        path: PathBuf,
        #[source]
        source: rustls::Error,
    },
}

pub(crate) fn validate_tls_ca_file(config: &TlsConfig) -> Result<(), TlsCaFileError> {
    if !matches!(config.mode, TlsMode::VerifyCa | TlsMode::VerifyFull) {
        return Ok(());
    }
    let Some(path) = config.root_cert.as_deref() else {
        return Ok(());
    };
    read_tls_ca_file(path, true).map(drop)
}

pub(crate) fn read_tls_ca_file(path: &Path, verifies: bool) -> Result<Vec<u8>, TlsCaFileError> {
    let bytes = read_ca_file(path)?;
    if verifies {
        validate_certificates(path, &bytes)?;
    }
    Ok(bytes)
}

fn read_ca_file(path: &Path) -> Result<Vec<u8>, TlsCaFileError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|source| TlsCaFileError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    let metadata = file.metadata().map_err(|source| TlsCaFileError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(TlsCaFileError::NotRegular {
            path: path.to_path_buf(),
        });
    }
    if metadata.len() > MAX_CA_FILE_BYTES {
        return Err(TlsCaFileError::TooLarge {
            path: path.to_path_buf(),
        });
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_CA_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| TlsCaFileError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > MAX_CA_FILE_BYTES {
        return Err(TlsCaFileError::TooLarge {
            path: path.to_path_buf(),
        });
    }
    Ok(bytes)
}

fn validate_certificates(path: &Path, bytes: &[u8]) -> Result<(), TlsCaFileError> {
    let mut roots = RootCertStore::empty();
    let mut count = 0;
    for result in CertificateDer::pem_slice_iter(bytes) {
        let certificate = result.map_err(|source| TlsCaFileError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        roots
            .add(certificate)
            .map_err(|source| TlsCaFileError::InvalidCertificate {
                path: path.to_path_buf(),
                source,
            })?;
        count += 1;
    }
    if count == 0 {
        return Err(TlsCaFileError::Empty {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}
