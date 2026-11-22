use std::collections::BTreeMap;

use bookie_core::{QueryRows, Value};

#[derive(Default)]
pub(super) struct UndecodableCounts(BTreeMap<usize, BTreeMap<String, u64>>);

impl UndecodableCounts {
    pub fn observe(&mut self, rows: &QueryRows) {
        for row in rows.iter() {
            for (index, value) in row.iter().enumerate() {
                if let Value::Undecodable(name) = value {
                    let types = self.0.entry(index).or_default();
                    let count = match types.get_mut(name.as_str()) {
                        Some(count) => count,
                        None => types.entry(name.clone()).or_default(),
                    };
                    *count = count.saturating_add(1);
                }
            }
        }
    }
}

impl Drop for UndecodableCounts {
    fn drop(&mut self) {
        for (index, types) in &self.0 {
            for (name, count) in types {
                tracing::warn!(column_index = index + 1, type_name = ?name, count, "postgres column values could not be decoded; showing them as undecodable");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_values_by_column_and_type_across_pages() {
        let mut counts = UndecodableCounts::default();
        let page: QueryRows =
            vec![vec![Value::Undecodable("A".into()), Value::Undecodable("B".into())]; 100_000].into();
        counts.observe(&page);
        counts.observe(&page);
        assert_eq!(counts.0.len(), 2);
        assert_eq!(counts.0[&0]["A"], 200_000);
        assert_eq!(counts.0[&1]["B"], 200_000);
    }
}
