use std::collections::HashMap;

use bookie_core::{ColumnInfo, Value};

use super::{CHANGE_COLUMN, CompareError};

pub(crate) struct CommonColumns {
    names: Vec<String>,
    left: Vec<usize>,
    right: Vec<usize>,
    left_info: Vec<ColumnInfo>,
    key: Vec<usize>,
}

fn index_by_name(columns: &[ColumnInfo]) -> Result<HashMap<&str, usize>, CompareError> {
    let mut index = HashMap::new();
    for (position, column) in columns.iter().enumerate() {
        if index.insert(column.name.as_str(), position).is_some() {
            return Err(CompareError::AmbiguousColumns);
        }
    }
    Ok(index)
}

impl CommonColumns {
    pub(crate) fn resolve(left: &[ColumnInfo], right: &[ColumnInfo], keys: &[String]) -> Result<Self, CompareError> {
        let right_index = index_by_name(right)?;
        index_by_name(left)?;
        let mut common = Self {
            names: Vec::new(),
            left: Vec::new(),
            right: Vec::new(),
            left_info: Vec::new(),
            key: Vec::new(),
        };
        for (position, column) in left.iter().enumerate() {
            if let Some(&other) = right_index.get(column.name.as_str()) {
                common.names.push(column.name.clone());
                common.left.push(position);
                common.right.push(other);
                common.left_info.push(column.clone());
            }
        }
        if common.names.is_empty() {
            return Err(CompareError::NoCommonColumns);
        }
        for key in keys {
            let position = common
                .names
                .iter()
                .position(|name| name == key)
                .ok_or(CompareError::UnknownKeyColumn)?;
            common.key.push(position);
        }
        Ok(common)
    }

    pub(crate) fn left_indices(&self) -> &[usize] {
        &self.left
    }

    pub(crate) fn right_indices(&self) -> &[usize] {
        &self.right
    }

    pub(crate) fn key_left(&self) -> Vec<usize> {
        self.key.iter().map(|&position| self.left[position]).collect()
    }

    pub(crate) fn key_right(&self) -> Vec<usize> {
        self.key.iter().map(|&position| self.right[position]).collect()
    }

    pub(crate) fn project_left(&self, row: &[Value]) -> Vec<Value> {
        project(&self.left, row)
    }

    pub(crate) fn project_right(&self, row: &[Value]) -> Vec<Value> {
        project(&self.right, row)
    }

    pub(crate) fn diff_columns(&self) -> Vec<ColumnInfo> {
        let mut columns = vec![ColumnInfo {
            name: CHANGE_COLUMN.into(),
            data_type: "text".into(),
            nullable: false,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }];
        columns.extend(self.left_info.iter().map(|column| ColumnInfo {
            primary_key: false,
            ..column.clone()
        }));
        columns
    }

    pub(crate) fn only_left(&self, left: &[ColumnInfo]) -> Vec<String> {
        left.iter()
            .filter(|column| !self.names.contains(&column.name))
            .map(|column| column.name.clone())
            .collect()
    }

    pub(crate) fn only_right(&self, right: &[ColumnInfo]) -> Vec<String> {
        right
            .iter()
            .filter(|column| !self.names.contains(&column.name))
            .map(|column| column.name.clone())
            .collect()
    }
}

fn project(indices: &[usize], row: &[Value]) -> Vec<Value> {
    indices
        .iter()
        .map(|&index| row.get(index).cloned().unwrap_or(Value::Null))
        .collect()
}
