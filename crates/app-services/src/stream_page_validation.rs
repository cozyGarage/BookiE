use bookie_core::{ColumnInfo, QueryResult};

#[derive(Debug, PartialEq, Eq)]
pub enum PageAppendError {
    OutOfOrder,
    SchemaChanged,
    InvalidRowWidth,
}

pub fn validate_page(
    index: u64,
    next_page: u64,
    columns: Option<&[ColumnInfo]>,
    result: &QueryResult,
) -> Result<(), PageAppendError> {
    if index != next_page {
        return Err(PageAppendError::OutOfOrder);
    }
    if result.rows.iter().any(|row| row.len() != result.columns.len()) {
        return Err(PageAppendError::InvalidRowWidth);
    }
    if columns.is_some_and(|columns| columns != result.columns) {
        return Err(PageAppendError::SchemaChanged);
    }
    Ok(())
}

#[derive(Default)]
pub struct StreamPageIndex {
    next_page: u64,
    empty_pages: u64,
    row_counts: Vec<usize>,
    row_starts: Vec<usize>,
    total_rows: usize,
}

impl StreamPageIndex {
    pub fn accept(
        &mut self,
        index: u64,
        columns: Option<&[ColumnInfo]>,
        result: &QueryResult,
    ) -> Result<bool, PageAppendError> {
        validate_page(index, self.next_page, columns, result)?;
        let count = result.rows.len();
        self.row_counts.push(count);
        self.row_starts.push(self.total_rows.saturating_add(1));
        self.next_page = self.next_page.saturating_add(1);
        let empty = index > 0 && count == 0;
        if empty {
            self.empty_pages = self.empty_pages.saturating_add(1);
        } else {
            self.total_rows = self.total_rows.saturating_add(count);
        }
        Ok(empty)
    }

    pub fn next_page(&self) -> u64 {
        self.next_page
    }
    pub fn total_pages(&self) -> u64 {
        self.next_page.saturating_sub(self.empty_pages)
    }
    pub fn total_rows(&self) -> usize {
        self.total_rows
    }
    pub fn row_counts(&self) -> &[usize] {
        &self.row_counts
    }
    pub fn row_starts(&self) -> &[usize] {
        &self.row_starts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::Value;

    pub(super) fn page(name: &str, rows: Vec<Vec<Value>>) -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: name.into(),
                data_type: "BIGINT".into(),
                nullable: false,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            }],
            rows: rows.into(),
            truncated: false,
        }
    }

    #[test]
    fn validates_order_width_and_schema_before_accepting_a_page() {
        let first = page("id", vec![vec![Value::Int(1)]]);
        assert_eq!(validate_page(0, 0, None, &first), Ok(()));
        assert_eq!(
            validate_page(2, 1, Some(&first.columns), &first),
            Err(PageAppendError::OutOfOrder)
        );
        let wrong_width = page("id", vec![vec![]]);
        assert_eq!(
            validate_page(1, 1, Some(&first.columns), &wrong_width),
            Err(PageAppendError::InvalidRowWidth)
        );
        let other = page("other", vec![]);
        assert_eq!(
            validate_page(1, 1, Some(&first.columns), &other),
            Err(PageAppendError::SchemaChanged)
        );
        let empty = page("id", vec![]);
        assert_eq!(validate_page(1, 1, Some(&first.columns), &empty), Ok(()));
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use bookie_core::Value;

    #[test]
    fn rows_and_empty_pages_keep_their_positions() {
        let mut index = StreamPageIndex::default();
        let first = super::tests::page("id", vec![vec![Value::Int(1)], vec![Value::Int(2)]]);
        let empty = super::tests::page("id", vec![]);
        let last = super::tests::page("id", vec![vec![Value::Int(3)]]);
        assert_eq!(index.accept(0, None, &first), Ok(false));
        assert_eq!(index.accept(1, Some(&first.columns), &empty), Ok(true));
        assert_eq!(index.accept(2, Some(&first.columns), &last), Ok(false));
        assert_eq!(index.row_counts(), &[2, 0, 1]);
        assert_eq!(index.row_starts(), &[1, 3, 3]);
        assert_eq!(index.total_rows(), 3);
        assert_eq!(index.total_pages(), 2);
    }

    #[test]
    fn rejected_pages_leave_index_unchanged_and_empty_pages_keep_offsets() {
        let mut index = StreamPageIndex::default();
        let row = QueryResult {
            columns: vec![],
            rows: vec![vec![Value::Int(1)]].into(),
            truncated: false,
        };
        let empty = QueryResult {
            columns: vec![],
            rows: vec![].into(),
            truncated: false,
        };
        assert_eq!(index.accept(0, None, &row), Err(PageAppendError::InvalidRowWidth));
        assert_eq!(index.next_page(), 0);
        assert_eq!(index.accept(0, None, &empty), Ok(false));
        assert_eq!(index.accept(2, Some(&[]), &empty), Err(PageAppendError::OutOfOrder));
        assert_eq!(index.next_page(), 1);
        assert_eq!(index.accept(1, Some(&[]), &empty), Ok(true));
        assert_eq!(index.row_starts(), &[1, 1]);
        assert_eq!(index.row_counts(), &[0, 0]);
        assert_eq!(index.total_pages(), 1);
        assert_eq!(index.total_rows(), 0);
    }
}
