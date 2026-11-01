use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::StorageError;

const RENAMED_DIRECTORIES: [(&str, &str); 2] = [("tablepro", "bookie"), ("tablepro-devel", "bookie-devel")];

pub fn legacy_secret_schema(current: &str) -> String {
    current.replacen("com.bookie.linux", "com.tablepro.linux", 1)
}

pub fn migrate_legacy_state() -> Result<usize, StorageError> {
    let mut copied = 0;
    for base in state_bases() {
        copied += migrate_legacy_directories(&base)?;
    }
    Ok(copied)
}

fn state_bases() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = |variable: &str, fallback: &str| {
        std::env::var_os(variable)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home.as_ref().map(|home| home.join(fallback)))
    };
    [
        base("XDG_CONFIG_HOME", ".config"),
        base("XDG_DATA_HOME", ".local/share"),
        base("XDG_STATE_HOME", ".local/state"),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(crate) fn migrate_legacy_directories(base: &Path) -> Result<usize, StorageError> {
    let mut copied = 0;
    for (legacy_name, name) in RENAMED_DIRECTORIES {
        let legacy = base.join(legacy_name);
        let target = base.join(name);
        if !is_real_directory(&legacy) || fs::symlink_metadata(&target).is_ok() {
            continue;
        }
        let staging = base.join(format!(".{name}.migrating-{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        let result = copy_tree(&legacy, &staging).and_then(|()| fs::rename(&staging, &target));
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error.into());
        }
        copied += 1;
    }
    Ok(copied)
}

fn is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir(destination)?;
    fs::set_permissions(destination, fs::metadata(source)?.permissions())?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "legacy_state_tests.rs"]
mod tests;
