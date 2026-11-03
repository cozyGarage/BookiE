use std::collections::HashMap;

use super::DiffRow;
use super::digest::cells_digest;
use super::keyed::{AFTER, BEFORE};

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

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
            .filter(|&column| cells_digest(&before.values, &[column]) != cells_digest(&after.values, &[column]))
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
