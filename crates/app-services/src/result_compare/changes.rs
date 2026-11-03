use std::collections::HashMap;

use bookie_core::Value;

use super::DiffRow;
use super::keyed::{AFTER, BEFORE};
use crate::value_exact::exact_equal;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangedCells(HashMap<usize, Vec<usize>>);

impl ChangedCells {
    pub fn from_cells(cells: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut changed: HashMap<usize, Vec<usize>> = HashMap::new();
        for (row, column) in cells {
            changed.entry(row).or_default().push(column);
        }
        Self(changed)
    }

    pub fn contains(&self, row: usize, column: usize) -> bool {
        self.0.get(&row).is_some_and(|columns| columns.contains(&column))
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn cell_equal(before: &[Value], after: &[Value], column: usize) -> bool {
    match (before.get(column), after.get(column)) {
        (Some(a), Some(b)) => exact_equal(a, b),
        (None, None) => true,
        _ => false,
    }
}

pub(crate) fn changed_cells(rows: &[DiffRow]) -> ChangedCells {
    let mut changed = HashMap::new();
    for (index, pair) in rows.windows(2).enumerate() {
        let [before, after] = pair else { continue };
        if before.change != BEFORE || after.change != AFTER {
            continue;
        }
        let columns: Vec<usize> = (0..before.values.len())
            .filter(|&column| !cell_equal(&before.values, &after.values, column))
            .map(|column| column + 1)
            .collect();
        if columns.is_empty() {
            continue;
        }
        changed.insert(index, columns.clone());
        changed.insert(index + 1, columns);
    }
    ChangedCells(changed)
}
