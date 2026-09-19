//! Reading a delimited file into rows a governed INSERT can bind.
//!
//! Everything here is pure: no connection, no execution, no I/O beyond
//! reading a file the caller names. The file is untrusted input, so every
//! dimension it controls is bounded before anything is allocated from it.

mod cell;
mod csv_import;

pub use cell::{CellError, ColumnKind, CsvRowError, column_kind, parse_cell, row_to_values};
pub use csv_import::{
    CsvFormat, CsvImportOptions, CsvSheet, ImportError, MAX_COLUMNS, MAX_FIELD_BYTES, MAX_FILE_BYTES, MAX_IMPORT_ROWS,
    MAX_PREVIEW_ROWS, detect_format, read_csv, read_csv_file, suggest_mapping,
};
