use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::TokenPermissions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToken {
    pub id: Uuid,
    pub name: String,
    /// SHA-256 hex of the plaintext token. Plaintext is shown once at
    /// issuance and never stored.
    pub token_hash: String,
    pub permissions: TokenPermissions,
    #[serde(default)]
    pub connection_allowlist: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub revoked: bool,
}

pub struct TokenStore {
    path: PathBuf,
    cache: Mutex<TokenCache>,
}

const MAX_TOKEN_STORE_BYTES: u64 = 1024 * 1024;

struct TokenCache {
    tokens: HashMap<Uuid, McpToken>,
    stamp: Option<TokenFileStamp>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct TokenFileStamp {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    mode: u32,
}

struct ProcessLock {
    _file: File,
}

impl TokenStore {
    pub fn open_default() -> Result<Self, String> {
        Self::open(tokens_path()?)
    }

    pub fn open(path: PathBuf) -> Result<Self, String> {
        let path = normalized_store_path(path)?;
        let _lock = acquire_process_lock(&path)?;
        let tokens = load_tokens(&path)?;
        let stamp = token_file_stamp(&path)?;
        Ok(Self {
            path,
            cache: Mutex::new(TokenCache { tokens, stamp }),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn issue(
        &self,
        name: String,
        permissions: TokenPermissions,
        connection_allowlist: Vec<Uuid>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(McpToken, String), String> {
        let plaintext = generate_token();
        let token = McpToken {
            id: Uuid::new_v4(),
            name,
            token_hash: hash_token(&plaintext),
            permissions,
            connection_allowlist,
            created_at: Utc::now(),
            expires_at,
            revoked: false,
        };
        let _lock = acquire_process_lock(&self.path)?;
        let mut map = load_tokens(&self.path)?;
        map.insert(token.id, token.clone());
        persist_tokens(&self.path, &map)?;
        let stamp = token_file_stamp(&self.path)?;
        let mut cache = self.cache.lock().map_err(|error| error.to_string())?;
        cache.tokens = map;
        cache.stamp = stamp;
        Ok((token, plaintext))
    }

    pub fn authenticate(&self, plaintext: &str) -> Result<McpToken, String> {
        let hash = hash_token(plaintext);
        let _lock = acquire_process_lock(&self.path)?;
        let refreshed = load_tokens(&self.path)?;
        let stamp = token_file_stamp(&self.path)?;
        let mut cache = self.cache.lock().map_err(|error| error.to_string())?;
        cache.tokens = refreshed;
        cache.stamp = stamp;
        let token = cache
            .tokens
            .values()
            .find(|t| !t.revoked && hashes_match(&t.token_hash, &hash))
            .cloned()
            .ok_or_else(|| "invalid token".to_string())?;
        if let Some(exp) = token.expires_at
            && exp < Utc::now()
        {
            return Err("token expired".into());
        }
        Ok(token)
    }

    pub(crate) fn has_live_candidate(&self, plaintext: &str) -> Result<bool, String> {
        self.refresh_cache_if_stale()?;
        let hash = hash_token(plaintext);
        let cache = self.cache.lock().map_err(|error| error.to_string())?;
        let now = Utc::now();
        Ok(cache.tokens.values().any(|token| {
            !token.revoked
                && token.expires_at.as_ref().is_none_or(|expires_at| expires_at >= &now)
                && hashes_match(&token.token_hash, &hash)
        }))
    }

    pub fn revoke(&self, id: Uuid) -> Result<(), String> {
        let _lock = acquire_process_lock(&self.path)?;
        let mut map = load_tokens(&self.path)?;
        if let Some(token) = map.get_mut(&id) {
            token.revoked = true;
        }
        persist_tokens(&self.path, &map)?;
        let stamp = token_file_stamp(&self.path)?;
        let mut cache = self.cache.lock().map_err(|error| error.to_string())?;
        cache.tokens = map;
        cache.stamp = stamp;
        Ok(())
    }

    pub fn list(&self) -> Vec<McpToken> {
        self.cache
            .lock()
            .map(|cache| cache.tokens.values().cloned().collect())
            .unwrap_or_default()
    }

    fn refresh_cache_if_stale(&self) -> Result<(), String> {
        for _ in 0..2 {
            let stamp = token_file_stamp(&self.path)?;
            if self.cache.lock().map_err(|error| error.to_string())?.stamp == stamp {
                return Ok(());
            }
            let tokens = load_tokens(&self.path)?;
            let refreshed_stamp = token_file_stamp(&self.path)?;
            if stamp == refreshed_stamp {
                let mut cache = self.cache.lock().map_err(|error| error.to_string())?;
                cache.tokens = tokens;
                cache.stamp = refreshed_stamp;
                return Ok(());
            }
        }
        Err("token store changed during refresh".into())
    }
}

fn token_file_stamp(path: &Path) -> Result<Option<TokenFileStamp>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot inspect token store {}: {error}", path.display())),
    };
    Ok(Some(TokenFileStamp {
        device: metadata.dev(),
        inode: metadata.ino(),
        size: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        mode: metadata.mode(),
    }))
}

fn normalized_store_path(path: PathBuf) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map_err(|error| error.to_string())?.join(path)
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| "token store path has no parent".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let parent = parent.canonicalize().map_err(|error| error.to_string())?;
    let name = absolute
        .file_name()
        .ok_or_else(|| "token store path has no file name".to_string())?;
    Ok(parent.join(name))
}

fn acquire_process_lock(path: &Path) -> Result<ProcessLock, String> {
    let lock_path = lock_path(path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&lock_path)
        .map_err(|error| format!("cannot open token store lock {}: {error}", lock_path.display()))?;
    validate_opened_file(&file, &lock_path)?;
    file.lock()
        .map_err(|error| format!("cannot lock token store {}: {error}", path.display()))?;
    Ok(ProcessLock { _file: file })
}

fn lock_path(path: &Path) -> Result<PathBuf, String> {
    let mut name = OsString::from(
        path.file_name()
            .ok_or_else(|| "token store path has no file name".to_string())?,
    );
    name.push(".lock");
    Ok(path.with_file_name(name))
}

fn load_tokens(path: &Path) -> Result<HashMap<Uuid, McpToken>, String> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(format!("cannot open token store {}: {error}", path.display())),
    };
    let metadata = validate_opened_file(&file, path)?;
    if metadata.len() > MAX_TOKEN_STORE_BYTES {
        return Err(token_store_size_error(path));
    }
    let mut bytes = Vec::new();
    file.take(MAX_TOKEN_STORE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read token store {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_TOKEN_STORE_BYTES {
        return Err(token_store_size_error(path));
    }
    let list: Vec<McpToken> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{} is not a readable token store: {error}", path.display()))?;
    Ok(list.into_iter().map(|token| (token.id, token)).collect())
}

fn persist_tokens(path: &Path, tokens: &HashMap<Uuid, McpToken>) -> Result<(), String> {
    let list = tokens.values().cloned().collect::<Vec<_>>();
    let json = serde_json::to_vec_pretty(&list).map_err(|error| error.to_string())?;
    if json.len() as u64 > MAX_TOKEN_STORE_BYTES {
        return Err(token_store_size_error(path));
    }
    let mut temporary_name = OsString::from(
        path.file_name()
            .ok_or_else(|| "token store path has no file name".to_string())?,
    );
    temporary_name.push(format!(".tmp.{}", Uuid::new_v4()));
    let temporary_path = path.with_file_name(temporary_name);
    let result = persist_through_temporary(path, &temporary_path, &json);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

fn persist_through_temporary(path: &Path, temporary_path: &Path, json: &[u8]) -> Result<(), String> {
    let mut handle = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(temporary_path)
        .map_err(|error| error.to_string())?;
    handle
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    validate_opened_file(&handle, temporary_path)?;
    handle.write_all(json).map_err(|error| error.to_string())?;
    handle.sync_all().map_err(|error| error.to_string())?;
    drop(handle);
    std::fs::rename(temporary_path, path).map_err(|error| error.to_string())?;
    File::open(
        path.parent()
            .ok_or_else(|| "token store path has no parent".to_string())?,
    )
    .and_then(|directory| directory.sync_all())
    .map_err(|error| error.to_string())
}

fn token_store_size_error(path: &Path) -> String {
    format!(
        "{} exceeds the maximum token store size of {MAX_TOKEN_STORE_BYTES} bytes",
        path.display()
    )
}

fn validate_opened_file(file: &File, path: &Path) -> Result<std::fs::Metadata, String> {
    let metadata = file
        .metadata()
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} must be a regular file", path.display()));
    }
    let effective_uid = unsafe { libc::geteuid() };
    if metadata.uid() != effective_uid {
        return Err(format!("{} must be owned by the current user", path.display()));
    }
    let mode = metadata.mode() & 0o7777;
    if mode != 0o600 {
        return Err(format!(
            "{} must have mode 0600 and be accessible only by its owner",
            path.display()
        ));
    }
    Ok(metadata)
}

fn hashes_match(stored: &str, candidate: &str) -> bool {
    let stored = stored.as_bytes();
    let candidate = candidate.as_bytes();
    if stored.len() != candidate.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in stored.iter().zip(candidate) {
        difference |= left ^ right;
    }
    difference == 0
}

pub fn generate_token() -> String {
    let raw = Uuid::new_v4().to_string() + &Uuid::new_v4().to_string();
    format!("tp_{}", &hex::encode(Sha256::digest(raw.as_bytes()))[..40])
}

fn hash_token(plaintext: &str) -> String {
    hex::encode(Sha256::digest(plaintext.as_bytes()))
}

fn tokens_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".config");
                p
            })
        })
        .ok_or_else(|| "neither XDG_CONFIG_HOME nor HOME is set".to_string())?;
    Ok(base.join("tablepro").join("mcp-tokens.json"))
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
