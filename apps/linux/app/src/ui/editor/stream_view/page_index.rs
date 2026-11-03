#[derive(Default)]
pub(super) struct PageIndex {
    ends: Vec<usize>,
}

impl PageIndex {
    pub(super) fn new(counts: &[usize]) -> Self {
        let mut total = 0_usize;
        let ends = counts
            .iter()
            .map(|count| {
                total = total.saturating_add(*count);
                total
            })
            .collect();
        Self { ends }
    }

    pub(super) fn row(&self, row: usize) -> Option<(u64, usize)> {
        let page = self.ends.partition_point(|end| *end <= row);
        let end = *self.ends.get(page)?;
        let start = page.checked_sub(1).map_or(0, |previous| self.ends[previous]);
        (row < end).then_some((page as u64, row - start))
    }

    pub(super) fn len(&self) -> usize {
        self.ends.last().copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rows_across_short_and_empty_pages() {
        let index = PageIndex::new(&[2, 0, 1, 3]);
        assert_eq!(index.len(), 6);
        assert_eq!(index.row(0), Some((0, 0)));
        assert_eq!(index.row(1), Some((0, 1)));
        assert_eq!(index.row(2), Some((2, 0)));
        assert_eq!(index.row(3), Some((3, 0)));
        assert_eq!(index.row(5), Some((3, 2)));
        assert_eq!(index.row(6), None);
    }

    #[test]
    fn maps_many_pages_without_a_per_row_index() {
        let counts = vec![5_000; 4_096];
        let index = PageIndex::new(&counts);
        assert_eq!(index.len(), 20_480_000);
        assert_eq!(index.row(20_479_999), Some((4_095, 4_999)));
    }
}
