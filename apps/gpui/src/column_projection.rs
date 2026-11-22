use std::collections::HashSet;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ColumnProjection {
    source_indices: Vec<usize>,
}

impl ColumnProjection {
    pub(crate) fn new(column_count: usize, hidden: &HashSet<usize>) -> Self {
        let mut source_indices: Vec<_> = (0..column_count)
            .filter(|index| !hidden.contains(index))
            .collect();
        if source_indices.is_empty() && column_count > 0 {
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

    #[test]
    fn hidden_columns_keep_stable_source_identity_and_can_be_restored() {
        let hidden = HashSet::from([1]);
        let projection = ColumnProjection::new(3, &hidden);

        assert_eq!(projection.len(), 2);
        assert_eq!(projection.source_index(0), Some(0));
        assert_eq!(projection.source_index(1), Some(2));
        assert_eq!(projection.visible_index(2), Some(1));
        assert_eq!(projection.visible_index(1), None);

        let restored = ColumnProjection::new(3, &HashSet::new());
        assert_eq!(restored.source_index(1), Some(1));
        assert_eq!(restored.visible_index(1), Some(1));
    }

    #[test]
    fn stale_preferences_cannot_hide_every_column() {
        let hidden = HashSet::from([0, 1, 2]);
        let projection = ColumnProjection::new(3, &hidden);

        assert_eq!(projection.len(), 1);
        assert_eq!(projection.source_index(0), Some(0));
    }

    #[test]
    fn hiding_one_duplicate_name_keeps_the_other_column() {
        let names = ["id", "id", "name"];
        let projection = ColumnProjection::new(names.len(), &HashSet::from([1]));

        assert_eq!(projection.len(), 2);
        let visible_names = (0..projection.len())
            .filter_map(|index| projection.source_index(index).map(|source| names[source]))
            .collect::<Vec<_>>();
        assert_eq!(visible_names, ["id", "name"]);
    }
}
