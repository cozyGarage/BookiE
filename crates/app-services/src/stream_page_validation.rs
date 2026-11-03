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

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::Value;

    fn page(name: &str, rows: Vec<Vec<Value>>) -> QueryResult {
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
