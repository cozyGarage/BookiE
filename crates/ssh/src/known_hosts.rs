use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use russh::keys::known_hosts::{check_known_hosts_path, known_host_keys_path, learn_known_hosts_path};
use russh::keys::ssh_key::PublicKey;

use crate::UnknownHostKey;
use crate::handshake::HostKeyPromptEvent;
use crate::openssh::{AskpassPrompt, PromptAnswer, Prompter};

#[derive(Debug, Clone)]
pub(super) enum HostKeyOutcome {
    Trusted,
    LearnedNew { fingerprint: String },
    Changed { fingerprint: String, line: usize },
    Unknown { fingerprint: String },
    KnownHostsIo(String),
}

pub(super) fn verify_or_learn(
    host: &str,
    port: u16,
    key: &PublicKey,
    known_hosts: &Path,
    fingerprint: &str,
    unknown: UnknownHostKey,
) -> HostKeyOutcome {
    match check_known_host(host, port, key, known_hosts, fingerprint) {
        Ok(true) => HostKeyOutcome::Trusted,
        Ok(false) if unknown == UnknownHostKey::Refuse => HostKeyOutcome::Unknown {
            fingerprint: fingerprint.to_string(),
        },
        Ok(false) => record_known_host(host, port, key, known_hosts, fingerprint),
        Err(outcome) => outcome,
    }
}

pub(super) async fn verify_or_prompt(
    host: &str,
    port: u16,
    key: &PublicKey,
    known_hosts: &Path,
    fingerprint: &str,
    prompter: &dyn Prompter,
    prompt_events: &tokio::sync::mpsc::UnboundedSender<HostKeyPromptEvent>,
) -> HostKeyOutcome {
    match check_known_host(host, port, key, known_hosts, fingerprint) {
        Ok(true) => HostKeyOutcome::Trusted,
        Ok(false) => {
            let prompt = AskpassPrompt::HostKeyConfirmation {
                host: format!("{host}:{port}"),
                algorithm: key.algorithm().to_string(),
                fingerprint: fingerprint.to_string(),
            };
            let _ = prompt_events.send(HostKeyPromptEvent::WaitingForUser);
            let answer = prompter.answer(&prompt).await;
            let outcome = if matches!(answer, PromptAnswer::Accept) {
                record_known_host(host, port, key, known_hosts, fingerprint)
            } else {
                HostKeyOutcome::Unknown {
                    fingerprint: fingerprint.to_string(),
                }
            };
            let declined_fingerprint = match &outcome {
                HostKeyOutcome::Unknown { fingerprint } => Some(fingerprint.clone()),
                _ => None,
            };
            tracing::warn!(
                declined = declined_fingerprint.is_some(),
                "SSH host key prompt answered"
            );
            let _ = prompt_events.send(HostKeyPromptEvent::UserResponded { declined_fingerprint });
            outcome
        }
        Err(outcome) => outcome,
    }
}

fn check_known_host(
    host: &str,
    port: u16,
    key: &PublicKey,
    known_hosts: &Path,
    fingerprint: &str,
) -> Result<bool, HostKeyOutcome> {
    match is_revoked(host, port, key, known_hosts) {
        Ok(false) => {}
        Ok(true) => {
            return Err(HostKeyOutcome::KnownHostsIo(format!(
                "the host key of {host}:{port} is marked @revoked in {}; refusing to connect",
                known_hosts.display()
            )));
        }
        Err(error) => return Err(HostKeyOutcome::KnownHostsIo(error)),
    }
    match check_known_hosts_path(host, port, key, known_hosts) {
        Ok(true) => Ok(true),
        Ok(false) => match known_host_keys_path(host, port, known_hosts) {
            Ok(existing) if !existing.is_empty() => Err(HostKeyOutcome::Changed {
                fingerprint: fingerprint.to_string(),
                line: existing[0].0,
            }),
            Ok(_) => Ok(false),
            Err(error) => Err(HostKeyOutcome::KnownHostsIo(error.to_string())),
        },
        Err(russh::keys::Error::KeyChanged { line }) => Err(HostKeyOutcome::Changed {
            fingerprint: fingerprint.to_string(),
            line,
        }),
        Err(error) => Err(HostKeyOutcome::KnownHostsIo(error.to_string())),
    }
}

struct ScratchFile(PathBuf);

impl ScratchFile {
    fn create(contents: &str) -> std::io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let path = std::env::temp_dir().join(format!("tablepro-revoked-{}", uuid::Uuid::new_v4().simple()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let scratch = Self(path);
        file.write_all(contents.as_bytes())?;
        Ok(scratch)
    }
}

impl Drop for ScratchFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn is_revoked(host: &str, port: u16, key: &PublicKey, known_hosts: &Path) -> Result<bool, String> {
    let text = match std::fs::read_to_string(known_hosts) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    let revoked: String = text
        .lines()
        .filter_map(|line| {
            line.trim_start()
                .strip_prefix("@revoked")
                .filter(|rest| rest.starts_with(char::is_whitespace))
        })
        .map(|rest| format!("{}\n", rest.trim_start()))
        .collect();
    if revoked.is_empty() {
        return Ok(false);
    }
    if revoked.lines().any(|line| revoked_by_pattern(line, host, port, key)) {
        return Ok(true);
    }
    let scratch = ScratchFile::create(&revoked).map_err(|error| error.to_string())?;
    let keys = known_host_keys_path(host, port, &scratch.0).map_err(|error| error.to_string())?;
    Ok(keys.iter().any(|(_, known)| known == key))
}

fn revoked_by_pattern(line: &str, host: &str, port: u16, key: &PublicKey) -> bool {
    let mut fields = line.split_whitespace();
    let (Some(patterns), Some(_), Some(base64)) = (fields.next(), fields.next(), fields.next()) else {
        return false;
    };
    if patterns.starts_with('|') || !host_patterns_match(patterns, host, port) {
        return false;
    }
    russh::keys::parse_public_key_base64(base64).is_ok_and(|revoked| &revoked == key)
}

fn host_patterns_match(patterns: &str, host: &str, port: u16) -> bool {
    let host = host.to_ascii_lowercase();
    let name = if port == 22 { host } else { format!("[{host}]:{port}") };
    let mut matched = false;
    for pattern in patterns.split(',').map(str::to_ascii_lowercase) {
        match pattern.strip_prefix('!') {
            Some(negated) if wildcard_match(negated.as_bytes(), name.as_bytes()) => return false,
            Some(_) => {}
            None => matched |= wildcard_match(pattern.as_bytes(), name.as_bytes()),
        }
    }
    matched
}

fn wildcard_match(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some(b'*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == b'?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p.min(pattern.len())..].iter().all(|c| *c == b'*')
}

fn record_known_host(host: &str, port: u16, key: &PublicKey, known_hosts: &Path, fingerprint: &str) -> HostKeyOutcome {
    match ensure_parent_dir(known_hosts).and_then(|_| {
        learn_known_hosts_path(host, port, key, known_hosts).map_err(|e| std::io::Error::other(format!("{e}")))
    }) {
        Ok(()) => HostKeyOutcome::LearnedNew {
            fingerprint: fingerprint.to_string(),
        },
        Err(error) => HostKeyOutcome::KnownHostsIo(error.to_string()),
    }
}

fn ensure_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn default_known_hosts_path() -> Option<PathBuf> {
    known_hosts_path_from(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"))
}

fn known_hosts_path_from(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = xdg_config_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("tablepro").join("known_hosts"))
}

#[cfg(test)]
#[path = "known_hosts_tests.rs"]
mod tests;
