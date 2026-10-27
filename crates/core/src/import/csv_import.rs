use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use thiserror::Error;

use crate::export::CsvDelimiter;
use crate::query::ColumnInfo;

/// Largest file the import will read into memory. A delimited file is
/// read whole because the mapping dialog and the row budget both need
/// the row count before the first statement runs.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

pub const MAX_COLUMNS: usize = 512;

// Hex-escaped binary doubles in CSV and adds its `\\x` prefix, so this admits
// a 1 MiB binary value while keeping each untrusted field bounded.
pub const MAX_FIELD_BYTES: usize = 2 * 1024 * 1024 + 2;

pub const MAX_IMPORT_ROWS: usize = 1_000_000;

/// Rows the mapping dialog reads before the user commits to anything.
pub const MAX_PREVIEW_ROWS: usize = 50;

/// Lines the format detector looks at. Enough to tell a delimiter from a
/// character that happens to appear in the first row.
const DETECT_LINES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportError {
    #[error("the file is {size} bytes, over the {limit} byte limit")]
    FileTooLarge { size: u64, limit: u64 },

    #[error("the file could not be opened")]
    Unreadable,

    #[error("the file is not valid UTF-8 at byte {offset}")]
    NotUtf8 { offset: usize },

    #[error("the file is not a readable .xlsx workbook")]
    NotSpreadsheet,
    #[error("the file is not valid {encoding} text")]
    NotDecodable { encoding: &'static str },

    #[error("line {line} could not be read as delimited text")]
    Malformed { line: u64 },

    #[error("the file has no columns in it")]
    NoColumns,

    #[error("the file has {columns} columns, over the {limit} column limit")]
    TooManyColumns { columns: usize, limit: usize },

    #[error("the file has more than {limit} rows")]
    TooManyRows { limit: usize },

    #[error("a field on line {line} is over the {limit} byte limit")]
    FieldTooLong { line: usize, limit: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvFormat {
    pub delimiter: CsvDelimiter,
    pub has_header: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CsvImportQuote {
    #[default]
    Double,
    Single,
    None,
}

impl CsvImportQuote {
    pub const ALL: [Self; 3] = [Self::Double, Self::Single, Self::None];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CsvImportEncoding {
    #[default]
    Utf8,
    Windows1252,
    Utf16Le,
    Utf16Be,
}

impl CsvImportEncoding {
    pub const ALL: [Self; 4] = [Self::Utf8, Self::Windows1252, Self::Utf16Le, Self::Utf16Be];

    fn codec(self) -> &'static encoding_rs::Encoding {
        match self {
            Self::Utf8 => encoding_rs::UTF_8,
            Self::Windows1252 => encoding_rs::WINDOWS_1252,
            Self::Utf16Le => encoding_rs::UTF_16LE,
            Self::Utf16Be => encoding_rs::UTF_16BE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvImportOptions {
    pub delimiter: CsvDelimiter,
    pub quote: CsvImportQuote,
    pub backslash_escape: bool,
    pub encoding: CsvImportEncoding,
    /// Whether the first record names the columns rather than holding data.
    pub has_header: bool,
    /// The text standing for NULL. Empty by default, which is what every
    /// export in this app writes for a NULL.
    pub null_marker: String,
}

impl Default for CsvImportOptions {
    fn default() -> Self {
        Self {
            delimiter: CsvDelimiter::Comma,
            quote: CsvImportQuote::default(),
            backslash_escape: false,
            encoding: CsvImportEncoding::default(),
            has_header: true,
            null_marker: String::new(),
        }
    }
}

impl From<CsvFormat> for CsvImportOptions {
    fn from(format: CsvFormat) -> Self {
        Self {
            delimiter: format.delimiter,
            has_header: format.has_header,
            ..Self::default()
        }
    }
}

/// A delimited file as the import reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CsvSheet {
    /// One name per field, from the header record or made up from the
    /// position when there is none.
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Rows past `limit` when the read was a preview, so the dialog can
    /// say there is more behind it.
    pub truncated: bool,
}

pub fn read_csv_file(path: &Path, options: &CsvImportOptions, limit: Option<usize>) -> Result<CsvSheet, ImportError> {
    let bytes = read_import_file(path)?;
    read_csv(&bytes, options, limit)
}

pub fn read_import_file(path: &Path) -> Result<Vec<u8>, ImportError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| ImportError::Unreadable)?;
    let metadata = file.metadata().map_err(|_| ImportError::Unreadable)?;
    if !metadata.is_file() {
        return Err(ImportError::Unreadable);
    }
    check_file_size(metadata.len())?;
    read_bounded(file, MAX_FILE_BYTES)
}

fn read_bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>, ImportError> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ImportError::Unreadable)?;
    let size = bytes.len() as u64;
    if size > limit {
        return Err(ImportError::FileTooLarge { size, limit });
    }
    Ok(bytes)
}

fn check_file_size(size: u64) -> Result<(), ImportError> {
    if size > MAX_FILE_BYTES {
        return Err(ImportError::FileTooLarge {
            size,
            limit: MAX_FILE_BYTES,
        });
    }
    Ok(())
}

/// Read `bytes` as delimited text. `limit` caps the rows returned, for the
/// preview the mapping dialog draws.
///
/// Ragged records are kept rather than refused: a short row is padded with
/// empty fields and a long one keeps its extra fields, because the mapping
/// decides which fields matter.
pub fn read_csv(bytes: &[u8], options: &CsvImportOptions, limit: Option<usize>) -> Result<CsvSheet, ImportError> {
    check_file_size(bytes.len() as u64)?;
    let decoded = decode_text(bytes, options.encoding)?;
    let mut reader = reader_for(decoded.as_bytes(), options);
    let headers = match options.has_header {
        true => record_strings(reader.headers().map_err(malformed)?, 1)?,
        false => Vec::new(),
    };
    check_width(headers.len())?;
    let (rows, truncated) = collect_rows(&mut reader, options, limit)?;
    finish_sheet(headers, rows, truncated)
}

fn decode_text(bytes: &[u8], encoding: CsvImportEncoding) -> Result<std::borrow::Cow<'_, str>, ImportError> {
    if encoding == CsvImportEncoding::Utf8 {
        return std::str::from_utf8(bytes)
            .map(std::borrow::Cow::Borrowed)
            .map_err(|error| ImportError::NotUtf8 {
                offset: error.valid_up_to(),
            });
    }
    let codec = encoding.codec();
    codec
        .decode_without_bom_handling_and_without_replacement(bytes)
        .ok_or(ImportError::NotDecodable { encoding: codec.name() })
}

fn reader_for<'a>(bytes: &'a [u8], options: &CsvImportOptions) -> csv::Reader<&'a [u8]> {
    let mut builder = csv::ReaderBuilder::new();
    builder
        .delimiter(delimiter_byte(options.delimiter))
        .has_headers(options.has_header)
        .flexible(true);
    match options.quote {
        CsvImportQuote::Double => builder.quote(b'"'),
        CsvImportQuote::Single => builder.quote(b'\''),
        CsvImportQuote::None => builder.quoting(false),
    };
    if options.backslash_escape {
        builder.escape(Some(b'\\'));
    }
    builder.from_reader(bytes)
}

fn collect_rows(
    reader: &mut csv::Reader<&[u8]>,
    options: &CsvImportOptions,
    limit: Option<usize>,
) -> Result<(Vec<Vec<String>>, bool), ImportError> {
    let header_offset = usize::from(options.has_header);
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (index, record) in reader.records().enumerate() {
        if let Some(limit) = limit
            && rows.len() >= limit
        {
            return Ok((rows, true));
        }
        if rows.len() >= MAX_IMPORT_ROWS {
            return Err(ImportError::TooManyRows { limit: MAX_IMPORT_ROWS });
        }
        let record = record.map_err(malformed)?;
        let line = index + header_offset + 1;
        check_width(record.len())?;
        rows.push(record_strings(&record, line)?);
    }
    Ok((rows, false))
}

fn finish_sheet(headers: Vec<String>, mut rows: Vec<Vec<String>>, truncated: bool) -> Result<CsvSheet, ImportError> {
    let width = headers.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if width == 0 {
        return Err(ImportError::NoColumns);
    }
    check_width(width)?;
    let headers = match headers.is_empty() {
        true => (1..=width).map(|index| format!("Column {index}")).collect(),
        false => headers,
    };
    for row in &mut rows {
        row.resize(width.max(row.len()), String::new());
    }
    Ok(CsvSheet {
        headers,
        rows,
        truncated,
    })
}

fn record_strings(record: &csv::StringRecord, line: usize) -> Result<Vec<String>, ImportError> {
    let mut out = Vec::with_capacity(record.len());
    for field in record.iter() {
        if field.len() > MAX_FIELD_BYTES {
            return Err(ImportError::FieldTooLong {
                line,
                limit: MAX_FIELD_BYTES,
            });
        }
        out.push(field.to_owned());
    }
    Ok(out)
}

fn check_width(width: usize) -> Result<(), ImportError> {
    if width > MAX_COLUMNS {
        return Err(ImportError::TooManyColumns {
            columns: width,
            limit: MAX_COLUMNS,
        });
    }
    Ok(())
}

fn malformed(error: csv::Error) -> ImportError {
    ImportError::Malformed {
        line: error.position().map_or(0, csv::Position::line),
    }
}

/// Guess the delimiter and whether the first record is a header. Both are
/// overridable; this only decides what the dialog opens with.
pub fn detect_format(bytes: &[u8]) -> CsvFormat {
    let delimiter = detect_delimiter(bytes);
    CsvFormat {
        delimiter,
        has_header: detect_header(bytes, delimiter),
    }
}

fn detect_delimiter(bytes: &[u8]) -> CsvDelimiter {
    let mut best = CsvDelimiter::Comma;
    let mut best_score = 0;
    for delimiter in CsvDelimiter::ALL {
        let score = delimiter_score(bytes, delimiter);
        if score > best_score {
            best = delimiter;
            best_score = score;
        }
    }
    best
}

/// A delimiter scores by how many fields it finds across the sample. A
/// character that is not the delimiter yields one field per record, which
/// scores zero and leaves the default in place.
fn delimiter_score(bytes: &[u8], delimiter: CsvDelimiter) -> usize {
    let records = sample_records(bytes, delimiter);
    let Some(width) = records.first().map(Vec::len) else {
        return 0;
    };
    if width < 2 {
        return 0;
    }
    // Every delimiter candidate sees the same CSV record boundaries; compare
    // sampled width directly instead of scaling each candidate by that count.
    width
}

fn sample_records(bytes: &[u8], delimiter: CsvDelimiter) -> Vec<Vec<String>> {
    let options = CsvImportOptions {
        delimiter,
        has_header: false,
        ..CsvImportOptions::default()
    };
    read_csv(bytes, &options, Some(DETECT_LINES)).map_or_else(|_| Vec::new(), |sheet| sheet.rows)
}

fn detect_header(bytes: &[u8], delimiter: CsvDelimiter) -> bool {
    let records = sample_records(bytes, delimiter);
    let Some(first) = records.first() else {
        return false;
    };
    if records.len() < 2 {
        return false;
    }
    first
        .iter()
        .all(|field| !field.trim().is_empty() && !looks_like_data(field))
}

fn looks_like_data(field: &str) -> bool {
    let trimmed = field.trim();
    trimmed.parse::<f64>().is_ok() || matches!(trimmed.to_ascii_lowercase().as_str(), "true" | "false" | "null")
}

/// A starting mapping: each table column takes the field whose header
/// matches its name, ignoring case and surrounding space. Columns the file
/// has no match for are left unmapped.
pub fn suggest_mapping(headers: &[String], columns: &[ColumnInfo]) -> Vec<Option<usize>> {
    let normalized_headers = headers
        .iter()
        .map(|header| header.trim().to_lowercase())
        .collect::<Vec<_>>();
    let normalized_columns = columns
        .iter()
        .map(|column| column.name.trim().to_lowercase())
        .collect::<Vec<_>>();
    let mut mapping = vec![None; columns.len()];
    let mut used_headers = vec![false; headers.len()];

    for (column_index, column) in columns.iter().enumerate() {
        let header_index = headers
            .iter()
            .enumerate()
            .find_map(|(index, header)| (!used_headers[index] && header.trim() == column.name.trim()).then_some(index));
        if let Some(header_index) = header_index {
            used_headers[header_index] = true;
            mapping[column_index] = Some(header_index);
        }
    }

    for (column_index, wanted) in normalized_columns.iter().enumerate() {
        if mapping[column_index].is_some() {
            continue;
        }
        let header_index = normalized_headers
            .iter()
            .enumerate()
            .find_map(|(index, header)| (!used_headers[index] && header == wanted).then_some(index));
        if let Some(header_index) = header_index {
            used_headers[header_index] = true;
            mapping[column_index] = Some(header_index);
        }
    }

    mapping
}

fn delimiter_byte(delimiter: CsvDelimiter) -> u8 {
    match delimiter {
        CsvDelimiter::Comma => b',',
        CsvDelimiter::Semicolon => b';',
        CsvDelimiter::Tab => b'\t',
        CsvDelimiter::Pipe => b'|',
    }
}

#[cfg(test)]
#[path = "csv_import_tests.rs"]
mod tests;
