use std::path::PathBuf;

use super::file::ResultFormat;
use super::supports_sql_literals;

impl ResultFormat {
    pub const ALL: [ResultFormat; 8] = [
        ResultFormat::Csv,
        ResultFormat::Json,
        ResultFormat::JsonLines,
        ResultFormat::Markdown,
        ResultFormat::Html,
        ResultFormat::Xml,
        ResultFormat::Sql,
        ResultFormat::Xlsx,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Json => "JSON",
            Self::JsonLines => "JSON Lines",
            Self::Markdown => "Markdown",
            Self::Html => "HTML",
            Self::Xml => "XML",
            Self::Sql => "SQL INSERT statements",
            Self::Xlsx => "Excel workbook",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::JsonLines => "jsonl",
            Self::Markdown => "md",
            Self::Html => "html",
            Self::Xml => "xml",
            Self::Sql => "sql",
            Self::Xlsx => "xlsx",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv",
            Self::Json => "application/json",
            Self::JsonLines => "application/x-ndjson",
            Self::Markdown => "text/markdown",
            Self::Html => "text/html",
            Self::Xml => "application/xml",
            Self::Sql => "application/sql",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    pub fn shows_csv_options(self) -> bool {
        self == Self::Csv
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        Some(match extension.to_ascii_lowercase().as_str() {
            "csv" => Self::Csv,
            "json" => Self::Json,
            "jsonl" | "ndjson" => Self::JsonLines,
            "md" | "markdown" => Self::Markdown,
            "html" | "htm" => Self::Html,
            "xml" => Self::Xml,
            "sql" => Self::Sql,
            "xlsx" => Self::Xlsx,
            _ => return None,
        })
    }
}

pub fn offered_formats(driver_id: &str, workbook_fits: bool) -> Vec<ResultFormat> {
    let statements = supports_sql_literals(driver_id);
    ResultFormat::ALL
        .into_iter()
        .filter(|format| statements || *format != ResultFormat::Sql)
        .filter(|format| workbook_fits || *format != ResultFormat::Xlsx)
        .collect()
}

pub fn suggested_file_name(name: &str, format: ResultFormat) -> String {
    let base = ResultFormat::ALL
        .iter()
        .find_map(|known| name.strip_suffix(&format!(".{}", known.extension())))
        .filter(|base| !base.is_empty())
        .unwrap_or(name);
    format!("{base}.{}", format.extension())
}

pub fn normalize_duplicate_extension(path: PathBuf, extension: &str) -> PathBuf {
    let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
        return path;
    };
    let duplicate_suffix = format!(".{extension}.{extension}");
    if !name.ends_with(&duplicate_suffix) {
        return path;
    }
    path.with_file_name(&name[..name.len() - extension.len() - 1])
}

pub fn insert_target(name: &str) -> (Option<String>, String) {
    match name.rsplit_once('.') {
        Some((schema, table)) if !schema.is_empty() && !table.is_empty() => {
            (Some(schema.to_string()), table.to_string())
        }
        _ => (None, name.to_string()),
    }
}
