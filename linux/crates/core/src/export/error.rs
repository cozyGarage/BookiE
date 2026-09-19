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
    #[error("An Excel worksheet holds at most {limit} rows and this result has {rows}")]
    WorkbookTooLarge { limit: usize, rows: usize },
    #[error("An Excel worksheet holds at most 16384 columns and this result has {columns}")]
    WorkbookColumnLimit { columns: usize },
    #[error("The Excel workbook could not be built")]
    Workbook(#[from] rust_xlsxwriter::XlsxError),
}

impl ExportError {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}
