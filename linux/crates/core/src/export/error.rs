use std::io;

use crate::sql_dialect::BuildSqlError;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("{0}")]
    Write(#[from] io::Error),
    #[error("The export was cancelled")]
    Cancelled,
    #[error("This connection cannot produce SQL statements")]
    SqlLiteralsUnsupported { driver_id: String },
    #[error("A SQL export needs the table the rows came from")]
    MissingSqlTarget,
    #[error(transparent)]
    Statement(#[from] BuildSqlError),
}

impl ExportError {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}
