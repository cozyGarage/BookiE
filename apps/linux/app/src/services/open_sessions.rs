use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use bookie_app_services::window_sessions::OpenSessionsRecord;
use uuid::Uuid;

use super::config_io::{atomic_write_json, xdg_config_path};

const FILE_NAME: &str = "open-sessions.json";

static WINDOWS: Mutex<WindowKeys> = Mutex::new(WindowKeys::new());
static PENDING_GROUPS: Mutex<VecDeque<(usize, Vec<Uuid>)>> = Mutex::new(VecDeque::new());
static FILE_LOCK: Mutex<u64> = Mutex::new(0);

struct WindowKeys {
    windows: Vec<(usize, Vec<Uuid>)>,
    generation: u64,
}

pub struct PendingSave {
    record: OpenSessionsRecord,
    generation: u64,
}

impl PendingSave {
    #[cfg(test)]
    pub fn record_windows(&self) -> &[Vec<Uuid>] {
        &self.record.windows
    }
}

impl WindowKeys {
    const fn new() -> Self {
        Self {
            windows: Vec::new(),
            generation: 0,
        }
    }

    fn release(&mut self, window: usize) {
        self.windows.retain(|(token, _)| *token != window);
    }

    fn record(&mut self, window: usize, keys: Vec<Uuid>) -> PendingSave {
        match self.windows.iter_mut().find(|(token, _)| *token == window) {
            Some((_, current)) => *current = keys,
            None => self.windows.push((window, keys)),
        }
        self.generation = self.generation.wrapping_add(1);
        PendingSave {
            record: OpenSessionsRecord::from_windows(self.windows.iter().map(|(_, keys)| keys.clone())),
            generation: self.generation,
        }
    }
}

pub fn record_window(window: usize, keys: Vec<Uuid>) -> PendingSave {
    WINDOWS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .record(window, keys)
}

pub fn queue_groups(groups: Vec<Vec<Uuid>>) -> Option<PendingSave> {
    let mut pending = PENDING_GROUPS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut windows = WINDOWS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut saved = None;
    for keys in groups {
        let placeholder = usize::MAX - pending.len();
        saved = Some(windows.record(placeholder, keys.clone()));
        pending.push_back((placeholder, keys));
    }
    saved
}

pub fn take_group() -> Option<Vec<Uuid>> {
    let (placeholder, keys) = PENDING_GROUPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .pop_front()?;
    WINDOWS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .release(placeholder);
    Some(keys)
}

pub fn stored() -> Option<Vec<u8>> {
    std::fs::read(xdg_config_path(FILE_NAME)?).ok()
}

pub fn save(pending: &PendingSave) -> Result<(), String> {
    let path = xdg_config_path(FILE_NAME).ok_or_else(|| "configuration path is unavailable".to_owned())?;
    save_to(&path, pending)
}

fn save_to(path: &Path, pending: &PendingSave) -> Result<(), String> {
    let mut written = FILE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if pending.generation <= *written {
        return Ok(());
    }
    atomic_write_json(path, &pending.record).map_err(|error| error.to_string())?;
    *written = pending.generation;
    Ok(())
}

#[cfg(test)]
mod tests {
    use bookie_app_services::window_sessions::{OpenSessionsLoad, parse_open_sessions};

    use super::*;

    fn stored_windows(path: &Path) -> Vec<Vec<Uuid>> {
        match parse_open_sessions(&std::fs::read(path).unwrap()) {
            OpenSessionsLoad::Restored(record) => record.windows,
            _ => Vec::new(),
        }
    }

    #[test]
    fn each_window_keeps_its_own_group_and_replaces_it_on_change() {
        let [a, b, c] = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let mut windows = WindowKeys::new();
        windows.record(1, vec![a]);
        windows.record(2, vec![b]);
        let pending = windows.record(1, vec![a, c]);
        assert_eq!(pending.record.windows, vec![vec![a, c], vec![b]]);
        let pending = windows.record(2, Vec::new());
        assert_eq!(pending.record.windows, vec![vec![a, c]]);
    }

    #[test]
    fn queued_groups_are_claimed_in_order_and_leave_the_record_once_claimed() {
        let [a, b] = [Uuid::new_v4(), Uuid::new_v4()];
        let queued = queue_groups(vec![vec![a], vec![b]]).unwrap();
        assert_eq!(queued.record.windows, vec![vec![a], vec![b]]);
        assert_eq!(take_group(), Some(vec![a]));
        let snapshot = record_window(1, Vec::new());
        assert_eq!(snapshot.record.windows, vec![vec![b]]);
        assert_eq!(take_group(), Some(vec![b]));
        assert_eq!(take_group(), None);
    }

    #[test]
    fn a_saved_record_reads_back_and_an_older_save_cannot_replace_a_newer_one() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        let [a, b] = [Uuid::new_v4(), Uuid::new_v4()];
        let base = *FILE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let older = PendingSave {
            record: OpenSessionsRecord::from_windows([vec![a]]),
            generation: base + 1,
        };
        let newer = PendingSave {
            record: OpenSessionsRecord::from_windows([vec![a, b]]),
            generation: base + 2,
        };
        save_to(&path, &newer).unwrap();
        save_to(&path, &older).unwrap();
        assert_eq!(stored_windows(&path), vec![vec![a, b]]);
    }
}
