use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config_io::xdg_config_path;
use crate::state_file::StateFile;

pub const MAX_DRAFTS: usize = 20;
pub const MAX_DRAFT_BYTES: usize = 256 * 1024;
const FILE: &str = "closed-drafts.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosedDraft {
    pub sql: String,
    pub connection: Option<Uuid>,
}

#[derive(Clone)]
pub struct ClosedDraftStore {
    file: Arc<StateFile<Vec<ClosedDraft>>>,
}

impl ClosedDraftStore {
    pub fn open() -> Option<Self> {
        Some(Self::at(xdg_config_path(FILE)?))
    }

    pub fn at(path: PathBuf) -> Self {
        Self {
            file: Arc::new(StateFile::load(path)),
        }
    }

    pub fn memory() -> Self {
        Self {
            file: Arc::new(StateFile::memory()),
        }
    }

    pub fn remember(&self, draft: ClosedDraft) -> Result<(), String> {
        if draft.sql.trim().is_empty() || draft.sql.len() > MAX_DRAFT_BYTES {
            return Ok(());
        }
        self.file.update(|drafts| {
            if drafts.last().is_some_and(|last| last.sql == draft.sql) {
                return;
            }
            drafts.push(draft);
            let excess = drafts.len().saturating_sub(MAX_DRAFTS);
            drafts.drain(..excess);
        })
    }

    pub fn take_latest(&self) -> Option<ClosedDraft> {
        let mut taken = None;
        self.file.update(|drafts| taken = drafts.pop()).ok()?;
        taken
    }

    pub fn forget_latest_if(&self, sql: &str) {
        let _ = self.file.update(|drafts| {
            if drafts.last().is_some_and(|last| last.sql == sql) {
                drafts.pop();
            }
        });
    }

    pub fn len(&self) -> usize {
        self.file.read(Vec::len).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn flush(&self) -> Result<(), String> {
        self.file.flush()
    }
}

#[cfg(test)]
#[path = "closed_drafts_tests.rs"]
mod tests;
