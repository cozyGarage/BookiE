use std::collections::HashMap;

use bookie_results::value_exact::exact_equal;

use crate::model::{Cell, ColumnMeta};
use crate::source::ResultSource;

#[derive(Debug, Clone, PartialEq)]
pub struct CellCheck {
    pub key: Vec<Cell>,
    pub column: usize,
    pub value: Cell,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SaveCheck {
    pub matched: usize,
    pub differ: usize,
    pub unseen: usize,
}

pub fn key_columns(columns: &[ColumnMeta]) -> Vec<usize> {
    columns
        .iter()
        .enumerate()
        .filter(|(_, column)| column.primary_key)
        .map(|(at, _)| at)
        .collect()
}

pub fn cell_checks<'a>(
    keys: &[usize],
    edits: impl IntoIterator<Item = (&'a [Cell], usize, &'a Cell)>,
) -> Vec<CellCheck> {
    if keys.is_empty() {
        return Vec::new();
    }
    edits
        .into_iter()
        .map(|(row, column, value)| CellCheck {
            key: keys
                .iter()
                .map(|at| row.get(*at).cloned().unwrap_or(Cell::Null))
                .collect(),
            column,
            value: value.clone(),
        })
        .collect()
}

fn fingerprint<'a>(cells: impl Iterator<Item = &'a Cell>) -> String {
    format!("{:?}", cells.collect::<Vec<_>>())
}

pub fn verify(source: &dyn ResultSource, keys: &[usize], checks: &[CellCheck]) -> SaveCheck {
    let mut wanted: HashMap<String, Vec<&CellCheck>> = HashMap::new();
    for check in checks {
        wanted.entry(fingerprint(check.key.iter())).or_default().push(check);
    }
    let mut result = SaveCheck::default();
    for index in 0..source.total() {
        if wanted.is_empty() {
            break;
        }
        let owned;
        let row = match source.row_ref(index) {
            Some(row) => row,
            None => {
                owned = source.row(index);
                &owned[..]
            }
        };
        let Some(found) = wanted.remove(&fingerprint(keys.iter().filter_map(|at| row.get(*at)))) else {
            continue;
        };
        for check in found {
            if row
                .get(check.column)
                .is_some_and(|read| exact_equal(read, &check.value))
            {
                result.matched += 1;
            } else {
                result.differ += 1;
            }
        }
    }
    result.unseen = wanted.values().map(Vec::len).sum();
    result
}

impl SaveCheck {
    pub fn warning(&self) -> Option<String> {
        if self.differ > 0 {
            return Some(format!(
                "{} saved cell(s) read back with a different value; the database changed them (a trigger, default or rounding). Check those rows before editing again.",
                self.differ
            ));
        }
        (self.unseen > 0).then(|| {
            format!(
                "{} saved row(s) are not in the reloaded rows, so they were not checked",
                self.unseen
            )
        })
    }
}

#[cfg(test)]
#[path = "save_check_tests.rs"]
mod tests;
