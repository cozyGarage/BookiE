use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub fn replace_private_file(path: &Path, temporary: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(temporary)?;
    let written = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(temporary, path)?;
        sync_parent(path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    written
}

pub fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)?.sync_all()
}

#[cfg(test)]
#[path = "atomic_file_tests.rs"]
mod tests;
