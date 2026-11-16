use std::future::Future;
use std::path::{Path, PathBuf};

use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub const MAX_QUEUED_FILES: usize = 20;
pub const MAX_RUN_ROWS: u64 = 2_000_000;
pub const MAX_RUN_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum QueueError {
    #[error("choose at least one file")]
    NoFiles,

    #[error("at most {max} files can be imported in one run")]
    TooManyFiles { max: usize },

    #[error("the files add up to more than {limit_mb} MB, which is the limit for one run")]
    RunTooLarge { limit_mb: u64 },
}

pub fn check_queue(files: &[(PathBuf, u64)]) -> Result<(), QueueError> {
    if files.is_empty() {
        return Err(QueueError::NoFiles);
    }
    if files.len() > MAX_QUEUED_FILES {
        return Err(QueueError::TooManyFiles { max: MAX_QUEUED_FILES });
    }
    let total = files.iter().fold(0u64, |sum, (_, bytes)| sum.saturating_add(*bytes));
    if total > MAX_RUN_FILE_BYTES {
        return Err(QueueError::RunTooLarge {
            limit_mb: MAX_RUN_FILE_BYTES / (1024 * 1024),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunBudget {
    pub remaining_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    Imported { rows: u64, skipped: u64 },
    Failed { rows: u64, message: String },
    Cancelled { rows: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileResult {
    Imported { rows: u64, skipped: u64 },
    Failed { rows: u64, message: String },
    Cancelled { rows: u64 },
    NotStarted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    pub path: PathBuf,
    pub result: FileResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueReport {
    pub files: Vec<FileReport>,
}

impl QueueReport {
    pub fn rows_written(&self) -> u64 {
        self.files.iter().fold(0u64, |sum, file| match &file.result {
            FileResult::Imported { rows, .. } | FileResult::Failed { rows, .. } | FileResult::Cancelled { rows } => {
                sum.saturating_add(*rows)
            }
            FileResult::NotStarted => sum,
        })
    }

    pub fn first_failure(&self) -> Option<&FileReport> {
        self.files
            .iter()
            .find(|file| matches!(file.result, FileResult::Failed { .. }))
    }

    pub fn was_cancelled(&self) -> bool {
        self.files
            .iter()
            .any(|file| matches!(file.result, FileResult::Cancelled { .. }))
    }

    pub fn succeeded(&self) -> bool {
        self.files
            .iter()
            .all(|file| matches!(file.result, FileResult::Imported { .. }))
    }

    pub fn summary(&self) -> String {
        self.files.iter().map(describe).collect::<Vec<_>>().join("\n")
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn describe(file: &FileReport) -> String {
    let name = file_name(&file.path);
    match &file.result {
        FileResult::Imported { rows, skipped: 0 } => crate::tr!("{file}: {rows} rows written")
            .replace("{file}", &name)
            .replace("{rows}", &rows.to_string()),
        FileResult::Imported { rows, skipped } => crate::tr!("{file}: {rows} rows written, {skipped} rows skipped")
            .replace("{file}", &name)
            .replace("{rows}", &rows.to_string())
            .replace("{skipped}", &skipped.to_string()),
        FileResult::Failed { rows, message } => crate::tr!("{file}: stopped after {rows} rows. {message}")
            .replace("{file}", &name)
            .replace("{rows}", &rows.to_string())
            .replace("{message}", message),
        FileResult::Cancelled { rows } => crate::tr!("{file}: cancelled after {rows} rows")
            .replace("{file}", &name)
            .replace("{rows}", &rows.to_string()),
        FileResult::NotStarted => crate::tr!("{file}: not started").replace("{file}", &name),
    }
}

pub trait FileImporter {
    fn import(
        &mut self,
        index: usize,
        count: usize,
        path: &Path,
        budget: RunBudget,
    ) -> impl Future<Output = FileOutcome> + Send;
}

pub async fn run_queue(
    paths: Vec<PathBuf>,
    token: &CancellationToken,
    importer: &mut impl FileImporter,
) -> QueueReport {
    let count = paths.len();
    let mut files: Vec<FileReport> = Vec::with_capacity(count);
    let mut rows_written = 0u64;
    let mut stop: Option<FileResult> = None;
    for (index, path) in paths.into_iter().enumerate() {
        if let Some(result) = remaining_result(&stop, token) {
            files.push(FileReport { path, result });
            continue;
        }
        let budget = RunBudget {
            remaining_rows: MAX_RUN_ROWS.saturating_sub(rows_written),
        };
        let outcome = importer.import(index, count, &path, budget).await;
        let result = match outcome {
            FileOutcome::Imported { rows, skipped } => FileResult::Imported { rows, skipped },
            FileOutcome::Failed { rows, message } => {
                stop = Some(FileResult::NotStarted);
                FileResult::Failed { rows, message }
            }
            FileOutcome::Cancelled { rows } => {
                stop = Some(FileResult::Cancelled { rows: 0 });
                FileResult::Cancelled { rows }
            }
        };
        if let FileResult::Imported { rows, .. } | FileResult::Failed { rows, .. } | FileResult::Cancelled { rows } =
            &result
        {
            rows_written = rows_written.saturating_add(*rows);
        }
        files.push(FileReport { path, result });
    }
    QueueReport { files }
}

fn remaining_result(stop: &Option<FileResult>, token: &CancellationToken) -> Option<FileResult> {
    if let Some(result) = stop {
        return Some(result.clone());
    }
    token.is_cancelled().then_some(FileResult::Cancelled { rows: 0 })
}

#[cfg(test)]
#[path = "import_queue_tests.rs"]
mod tests;
