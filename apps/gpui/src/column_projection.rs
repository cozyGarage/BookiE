use std::collections::HashSet;

/// Maps the displayed columns back to their stable positions in the result source.
///
/// GPUI table selection and editing use visible column indexes, while BookiE's
/// values, edits, copy, and find paths use source-column indexes. Keep that
/// translation explicit whenever a projection hides columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ColumnProjection {
    source_indices: Vec<usize>,
}

impl ColumnProjection {
    pub(crate) fn new(names: &[String], hidden: &HashSet<String>) -> Self {
        let mut source_indices: Vec<_> = names
            .iter()
            .enumerate()
            .filter_map(|(index, name)| (!hidden.contains(name)).then_some(index))
            .collect();
        // A corrupt or stale preference must never produce an empty grid.
        if source_indices.is_empty() && !names.is_empty() {
            source_indices.push(0);
        }
        Self { source_indices }
    }

    pub(crate) fn len(&self) -> usize {
        self.source_indices.len()
    }

    pub(crate) fn source_index(&self, visible_index: usize) -> Option<usize> {
        self.source_indices.get(visible_index).copied()
    }

    pub(crate) fn visible_index(&self, source_index: usize) -> Option<usize> {
        self.source_indices.iter().position(|&index| index == source_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["id", "name", "secret"]
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn hidden_columns_keep_stable_source_identity_and_can_be_restored() {
        let names = names();
        let hidden = HashSet::from(["name".to_string()]);
        let projection = ColumnProjection::new(&names, &hidden);

        assert_eq!(projection.len(), 2);
        assert_eq!(projection.source_index(0), Some(0));
        assert_eq!(projection.source_index(1), Some(2));
        assert_eq!(projection.visible_index(2), Some(1));
        assert_eq!(projection.visible_index(1), None);

        let restored = ColumnProjection::new(&names, &HashSet::new());
        assert_eq!(restored.source_index(1), Some(1));
        assert_eq!(restored.visible_index(1), Some(1));
    }

    #[test]
    fn stale_preferences_cannot_hide_every_column() {
        let names = names();
        let hidden = names.iter().cloned().collect();
        let projection = ColumnProjection::new(&names, &hidden);

        assert_eq!(projection.len(), 1);
        assert_eq!(projection.source_index(0), Some(0));
    }
}
