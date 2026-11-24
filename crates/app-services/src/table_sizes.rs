use std::collections::HashMap;

use bookie_core::{Connection, DriverError, OperationControl, TableSize};

const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SizeIndex {
    exact: HashMap<(Option<String>, String), u64>,
    by_name: HashMap<String, Option<u64>>,
}

impl SizeIndex {
    pub fn new(sizes: Vec<TableSize>) -> Self {
        let mut index = Self::default();
        for size in sizes {
            index
                .by_name
                .entry(size.name.clone())
                .and_modify(|unique| *unique = None)
                .or_insert(Some(size.bytes));
            index.exact.insert((size.schema, size.name), size.bytes);
        }
        index
    }

    pub fn get(&self, schema: Option<&str>, name: &str) -> Option<u64> {
        let exact = self.exact.get(&(schema.map(str::to_string), name.to_string())).copied();
        match (exact, schema) {
            (Some(bytes), _) => Some(bytes),
            (None, None) => self.by_name.get(name).copied().flatten(),
            (None, Some(_)) => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty()
    }
}

pub async fn load_table_sizes(
    connection: &dyn Connection,
    control: &OperationControl,
) -> Result<SizeIndex, DriverError> {
    connection
        .table_sizes_controlled(None, control)
        .await
        .map(SizeIndex::new)
}

pub fn size_label(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
#[path = "table_sizes_tests.rs"]
mod tests;
