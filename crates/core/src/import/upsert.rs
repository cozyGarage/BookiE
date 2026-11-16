use crate::driver::ImportUpsertStyle;
use crate::import::plan::{ImportTarget, InsertPlan, PlanError};
use crate::query::{ColumnInfo, IndexInfo};
use crate::sql_dialect::{quote_ident, validate_ident};

pub const MAX_CONFLICT_KEYS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictKey {
    pub label: String,
    pub columns: Vec<String>,
    pub primary: bool,
}

pub fn conflict_key_candidates(columns: &[ColumnInfo], indexes: &[IndexInfo]) -> Vec<ConflictKey> {
    let mut keys: Vec<ConflictKey> = Vec::new();
    let primary_from_columns: Vec<String> = columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| column.name.clone())
        .collect();
    let primary_index = indexes.iter().find(|index| index.primary && !index.columns.is_empty());
    let primary_columns = primary_index.map_or(primary_from_columns, |index| index.columns.clone());
    if !primary_columns.is_empty() {
        keys.push(ConflictKey {
            label: "Primary key".to_owned(),
            columns: primary_columns,
            primary: true,
        });
    }
    for index in indexes {
        if keys.len() >= MAX_CONFLICT_KEYS {
            break;
        }
        let usable = index.unique
            && !index.primary
            && index.predicate.is_none()
            && !index.has_expression_key
            && !index.columns.is_empty();
        if usable && !keys.iter().any(|key| key.columns == index.columns) {
            keys.push(ConflictKey {
                label: index.name.clone(),
                columns: index.columns.clone(),
                primary: false,
            });
        }
    }
    keys
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WriteMode {
    #[default]
    Insert,
    UpdateOnConflict(Vec<String>),
}

impl WriteMode {
    pub fn updates_existing_rows(&self) -> bool {
        matches!(self, Self::UpdateOnConflict(_))
    }

    pub fn apply(&self, plan: InsertPlan, target: &ImportTarget<'_>) -> Result<InsertPlan, PlanError> {
        match self {
            Self::Insert => Ok(plan),
            Self::UpdateOnConflict(key) => apply_upsert(plan, target, key),
        }
    }
}

pub fn apply_upsert(
    mut plan: InsertPlan,
    target: &ImportTarget<'_>,
    key_columns: &[String],
) -> Result<InsertPlan, PlanError> {
    let style = target.capabilities.import_upsert;
    if style == ImportUpsertStyle::Unsupported {
        return Err(PlanError::UpsertUnsupported);
    }
    if key_columns.is_empty() {
        return Err(PlanError::NoConflictKey);
    }
    for (index, name) in key_columns.iter().enumerate() {
        validate_ident(name)?;
        if key_columns[..index].contains(name) {
            return Err(PlanError::DuplicateConflictColumn(name.clone()));
        }
        if !plan.columns.iter().any(|column| &column.name == name) {
            return Err(PlanError::ConflictColumnNotImported(name.clone()));
        }
    }
    let assignments = update_assignments(style, target.driver_id, &plan.columns, key_columns);
    if assignments.is_empty() {
        return Err(PlanError::NothingToUpdate);
    }
    let suffix = match style {
        ImportUpsertStyle::OnConflict => {
            let keys = quoted_list(target.driver_id, key_columns);
            format!(" ON CONFLICT ({keys}) DO UPDATE SET {}", assignments.join(", "))
        }
        _ => format!(" ON DUPLICATE KEY UPDATE {}", assignments.join(", ")),
    };
    plan.statement.push_str(&suffix);
    Ok(plan)
}

fn quoted_list(driver_id: &str, names: &[String]) -> String {
    names
        .iter()
        .map(|name| quote_ident(driver_id, name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn update_assignments(
    style: ImportUpsertStyle,
    driver_id: &str,
    columns: &[ColumnInfo],
    key_columns: &[String],
) -> Vec<String> {
    columns
        .iter()
        .filter(|column| !key_columns.contains(&column.name))
        .map(|column| {
            let quoted = quote_ident(driver_id, &column.name);
            match style {
                ImportUpsertStyle::OnConflict => format!("{quoted} = EXCLUDED.{quoted}"),
                _ => format!("{quoted} = VALUES({quoted})"),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "upsert_tests.rs"]
mod tests;
