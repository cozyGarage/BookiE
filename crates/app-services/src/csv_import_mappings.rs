use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tablepro_core::{ColumnInfo, DriverCapabilities};
use uuid::Uuid;

use super::config_io::xdg_config_path;
use super::state_file::StateFile;

const FILE: &str = "csv_import_mappings.json";
const MAX_COLUMNS: usize = 512;
const MAX_SCOPES: usize = 2_000;
const MAX_FILE_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct CsvMappingStore {
    file: Arc<StateFile<Mappings>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CsvMappingScope {
    pub connection_id: Uuid,
    pub database: String,
    pub driver_id: String,
    pub capabilities: DriverCapabilities,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct Mappings(HashMap<String, StoredMapping>);

#[derive(Clone, Deserialize, Serialize)]
struct StoredMapping {
    columns: Vec<StoredColumn>,
}

#[derive(Clone, Deserialize, Serialize)]
struct StoredColumn {
    target: String,
    source: Option<StoredHeader>,
}

#[derive(Clone, Deserialize, Serialize)]
struct StoredHeader {
    name: String,
    occurrence: usize,
}

impl CsvMappingScope {
    fn storage_key(&self) -> String {
        serde_json::to_string(&(
            self.connection_id,
            &self.database,
            &self.driver_id,
            &self.schema,
            &self.table,
        ))
        .unwrap_or_default()
    }
}

impl CsvMappingStore {
    pub fn open() -> Option<Self> {
        Some(Self::load_at(xdg_config_path(FILE)?))
    }

    fn load_at(path: std::path::PathBuf) -> Self {
        Self {
            file: Arc::new(StateFile::load(path)),
        }
    }

    pub fn save(
        &self,
        scope: &CsvMappingScope,
        headers: &[String],
        columns: &[ColumnInfo],
        mapping: &[Option<usize>],
        has_header: bool,
    ) -> Result<(), String> {
        validate_scope(scope)?;
        validate_inputs(scope.capabilities, headers, columns, mapping, has_header)?;
        let stored = StoredMapping {
            columns: columns
                .iter()
                .zip(mapping)
                .map(|(column, source)| {
                    Ok(StoredColumn {
                        target: column.name.clone(),
                        source: source.map(|index| header_identity(headers, index)).transpose()?,
                    })
                })
                .collect::<Result<_, String>>()?,
        };
        if serde_json::to_vec(&stored).map_err(|error| error.to_string())?.len() > MAX_FILE_BYTES {
            return Err("This column mapping is too large to save.".into());
        }
        let mut refusal = None;
        self.file.update(|mappings| {
            let key = scope.storage_key();
            if mappings.0.len() >= MAX_SCOPES && !mappings.0.contains_key(&key) {
                refusal = Some("The saved column mapping limit has been reached.");
            } else {
                let mut next = mappings.clone();
                next.0.insert(key, stored);
                match serde_json::to_vec(&next) {
                    Ok(bytes) if bytes.len() <= MAX_FILE_BYTES => *mappings = next,
                    _ => refusal = Some("Saved column mappings reached their storage limit."),
                }
            }
        })?;
        if let Some(message) = refusal {
            return Err(message.into());
        }
        Ok(())
    }

    pub fn restore(
        &self,
        scope: &CsvMappingScope,
        headers: &[String],
        columns: &[ColumnInfo],
        has_header: bool,
    ) -> Result<Option<Vec<Option<usize>>>, String> {
        validate_scope(scope)?;
        if !has_header {
            return Err("Saved mappings require a header row.".into());
        }
        validate_dimensions(headers, columns)?;
        let Some(stored) = self
            .file
            .read(|mappings| mappings.0.get(&scope.storage_key()).cloned())
            .flatten()
        else {
            return Ok(None);
        };
        if stored.columns.len() > MAX_COLUMNS {
            return Err("The saved column mapping is invalid.".into());
        }
        let mut result = vec![None; columns.len()];
        let mut seen_targets = std::collections::HashSet::new();
        for stored_column in stored.columns {
            if stored_column.target.is_empty() || !seen_targets.insert(stored_column.target.clone()) {
                return Err("The saved column mapping has duplicate or empty destination names.".into());
            }
            let matches: Vec<usize> = columns
                .iter()
                .enumerate()
                .filter_map(|(index, column)| (column.name == stored_column.target).then_some(index))
                .collect();
            let [target] = matches.as_slice() else {
                return Err("A saved destination column is missing or ambiguous. Review the mapping manually.".into());
            };
            if let Some(source) = stored_column.source {
                if is_generated(&columns[*target])
                    || (columns[*target].is_auto_increment
                        && !scope.capabilities.supports_explicit_auto_increment_values)
                {
                    return Err("A saved mapping targets a column that cannot accept explicit values.".into());
                }
                result[*target] = Some(resolve_header(headers, &source)?);
            }
        }
        Ok(Some(result))
    }

    pub fn forget(&self, scope: &CsvMappingScope) -> Result<bool, String> {
        validate_scope(scope)?;
        let mut removed = false;
        self.file
            .update(|mappings| removed = mappings.0.remove(&scope.storage_key()).is_some())?;
        Ok(removed)
    }

    pub fn flush(&self) -> Result<(), String> {
        self.file.flush()
    }
}

fn validate_scope(scope: &CsvMappingScope) -> Result<(), String> {
    if scope.database.len() > 1024
        || scope.driver_id.len() > 128
        || scope.schema.as_ref().is_some_and(|schema| schema.len() > 1024)
        || scope.table.len() > 1024
    {
        return Err("The connection or table name is too long to save a mapping.".into());
    }
    Ok(())
}

fn validate_inputs(
    capabilities: DriverCapabilities,
    headers: &[String],
    columns: &[ColumnInfo],
    mapping: &[Option<usize>],
    has_header: bool,
) -> Result<(), String> {
    if !has_header {
        return Err("Saved mappings require a header row.".into());
    }
    validate_dimensions(headers, columns)?;
    if mapping.len() != columns.len() {
        return Err("The column mapping does not match the destination table.".into());
    }
    let mut targets = std::collections::HashSet::new();
    for (column, source) in columns.iter().zip(mapping) {
        if column.name.is_empty() || !targets.insert(&column.name) {
            return Err("Destination column names are empty or ambiguous.".into());
        }
        if source.is_some_and(|index| index >= headers.len())
            || (source.is_some()
                && (is_generated(column)
                    || (column.is_auto_increment && !capabilities.supports_explicit_auto_increment_values)))
        {
            return Err("The selected mapping is invalid for this table.".into());
        }
    }
    Ok(())
}

fn validate_dimensions(headers: &[String], columns: &[ColumnInfo]) -> Result<(), String> {
    if headers.len() > MAX_COLUMNS || columns.len() > MAX_COLUMNS {
        return Err("This table or CSV has too many columns to save a mapping.".into());
    }
    if headers.iter().any(|header| header.len() > 1024) {
        return Err("A CSV header is too long to save in a mapping.".into());
    }
    Ok(())
}

fn header_identity(headers: &[String], index: usize) -> Result<StoredHeader, String> {
    let name = headers.get(index).ok_or("The selected CSV column no longer exists.")?;
    let occurrence = headers[..=index].iter().filter(|header| *header == name).count();
    Ok(StoredHeader {
        name: name.clone(),
        occurrence,
    })
}

fn resolve_header(headers: &[String], source: &StoredHeader) -> Result<usize, String> {
    if source.occurrence == 0 {
        return Err("The saved CSV header reference is invalid.".into());
    }
    headers
        .iter()
        .enumerate()
        .filter(|(_, name)| **name == source.name)
        .nth(source.occurrence - 1)
        .map(|(index, _)| index)
        .ok_or_else(|| format!("CSV header '{}' is missing. Review the mapping manually.", source.name))
}

fn is_generated(column: &ColumnInfo) -> bool {
    column.is_generated
}

#[cfg(test)]
#[path = "csv_import_mappings_tests.rs"]
mod tests;
