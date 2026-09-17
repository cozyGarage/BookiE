use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub fn xdg_config_path(filename: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join(crate::config::storage_dir_name()).join(filename))
}

pub fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    atomic_write_bytes(path, &serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?)
}

pub fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(std::io::Error::other("settings path is not a regular file"));
        }
        Ok(_) => std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    glib::file_set_contents_full(
        path,
        bytes,
        glib::FileSetContentsFlags::CONSISTENT | glib::FileSetContentsFlags::DURABLE,
        0o600,
    )
    .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_are_private_and_do_not_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        atomic_write_json(&path, &vec![1]).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(atomic_write_json(&link, &vec![2]).is_err());
        assert_eq!(
            serde_json::from_slice::<Vec<i32>>(&std::fs::read(path).unwrap()).unwrap(),
            vec![1]
        );
    }
}
