// D-Bus registration cannot enforce uniqueness without a session bus.
// Keep flock held across shared state access; closing the file releases it.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;

pub struct Lock {
    // Keeping this file open keeps the flock held.
    _file: File,
}

#[derive(Debug)]
pub enum LockError {
    AlreadyRunning,
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::AlreadyRunning => write!(f, "another BookiE instance is already running"),
            LockError::Io(e) => write!(f, "single-instance lock io error: {e}"),
        }
    }
}

impl std::error::Error for LockError {}

fn lock_path() -> Option<PathBuf> {
    let lock_file = format!("{}.lock", crate::config::storage_dir_name());
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return Some(PathBuf::from(dir).join(lock_file));
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    let dir = cache.join(crate::config::storage_dir_name());
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(lock_file))
}

pub fn acquire() -> Result<Lock, LockError> {
    let Some(path) = lock_path() else {
        return Err(LockError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no XDG runtime / cache / HOME directory",
        )));
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(LockError::Io)?;
    // SAFETY: `file` keeps a valid descriptor open during the syscall.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(Lock { _file: file });
    }
    let err = std::io::Error::last_os_error();
    if matches!(err.raw_os_error(), Some(libc::EWOULDBLOCK)) {
        Err(LockError::AlreadyRunning)
    } else {
        Err(LockError::Io(err))
    }
}
pub fn raise_running_instance(app_id: &str) {
    use gtk4::gio::prelude::*;

    let app = gtk4::Application::new(Some(app_id), gtk4::gio::ApplicationFlags::HANDLES_OPEN);
    if app.register(gtk4::gio::Cancellable::NONE).is_err() || !app.is_remote() {
        tracing::info!("another BookiE instance is running and could not be reached over D-Bus; exiting");
        return;
    }
    tracing::info!("another BookiE instance is running; asked it to show its window");
    app.run_with_args(&std::env::args().collect::<Vec<_>>());
}
