use bookie_core::{ColumnInfo, TableInfo, Value};

use super::source_table::{SourceRefusal, SourceTable, source_columns_for_result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultLock {
    NotOneSelect,
    MoreThanOneTable,
    Collapsed,
    Untraced,
    NoPrimaryKey,
    KeyNotShown,
    ColumnsChanged,
}

impl From<SourceRefusal> for ResultLock {
    fn from(refusal: SourceRefusal) -> Self {
        match refusal {
            SourceRefusal::NotOneSelect => Self::NotOneSelect,
            SourceRefusal::MoreThanOneTable => Self::MoreThanOneTable,
            SourceRefusal::Collapsed => Self::Collapsed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultRefusal {
    NotOneSelect,
    MoreThanOneTable,
    Collapsed,
    Untraced,
    NoPrimaryKey,
    KeyNotShown,
    ColumnsChanged,
    ReadOnlySession,
    OpenTransaction,
    OtherDatabase,
    OtherResult,
    Checking,
    ColumnsUnread,
    MultiResult,
}

impl From<ResultLock> for ResultRefusal {
    fn from(lock: ResultLock) -> Self {
        match lock {
            ResultLock::NotOneSelect => Self::NotOneSelect,
            ResultLock::MoreThanOneTable => Self::MoreThanOneTable,
            ResultLock::Collapsed => Self::Collapsed,
            ResultLock::Untraced => Self::Untraced,
            ResultLock::NoPrimaryKey => Self::NoPrimaryKey,
            ResultLock::KeyNotShown => Self::KeyNotShown,
            ResultLock::ColumnsChanged => Self::ColumnsChanged,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditTarget {
    pub table: TableInfo,
    pub positions: Vec<Option<usize>>,
    pub catalog: Vec<ColumnInfo>,
}

pub fn editable_target(
    source: &SourceTable,
    result_columns: &[ColumnInfo],
    catalog: &[ColumnInfo],
) -> Result<EditTarget, ResultLock> {
    let names: Vec<String> = result_columns.iter().map(|column| column.name.clone()).collect();
    let parsed = source_columns_for_result(source, catalog, &names).ok_or(ResultLock::ColumnsChanged)?;
    let mut positions = Vec::with_capacity(result_columns.len());
    for mapped in parsed {
        let first = mapped.filter(|position| !positions.contains(&Some(*position)));
        positions.push(first);
    }
    let mut keys = catalog
        .iter()
        .enumerate()
        .filter(|(_, info)| info.primary_key)
        .peekable();
    if keys.peek().is_none() {
        return Err(ResultLock::NoPrimaryKey);
    }
    if keys.any(|(index, _)| !positions.contains(&Some(index))) {
        return Err(ResultLock::KeyNotShown);
    }
    Ok(EditTarget {
        table: source.table.clone(),
        positions,
        catalog: catalog.to_vec(),
    })
}

pub fn owned_result_columns(result_columns: &[ColumnInfo], target: &EditTarget) -> Vec<ColumnInfo> {
    let mut columns = result_columns.to_vec();
    for (column, position) in columns.iter_mut().zip(&target.positions) {
        let Some(position) = position else {
            column.is_generated = true;
            column.primary_key = false;
            continue;
        };
        let Some(known) = target.catalog.get(*position) else {
            continue;
        };
        if column.data_type.is_empty() || column.data_type.eq_ignore_ascii_case("NULL") {
            column.data_type.clone_from(&known.data_type);
        }
        column.is_generated |= known.is_generated;
        column.is_auto_increment |= known.is_auto_increment;
        column.primary_key |= known.primary_key;
        column.nullable = known.nullable;
        if column.default_value.is_none() {
            column.default_value.clone_from(&known.default_value);
        }
    }
    columns
}

pub fn mutation_columns(result_columns: &[ColumnInfo], target: &EditTarget) -> Vec<ColumnInfo> {
    let mut columns = owned_result_columns(result_columns, target);
    for (column, position) in columns.iter_mut().zip(&target.positions) {
        if let Some(source) = position.and_then(|position| target.catalog.get(position)) {
            *column = source.clone();
        }
    }
    columns
}

pub fn can_stage_result_value(column: &ColumnInfo, value: &Value) -> bool {
    !column.is_generated
        && !column.primary_key
        && !column.is_auto_increment
        && (!matches!(value, Value::Null) || column.nullable)
}
