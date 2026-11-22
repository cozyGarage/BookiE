use std::cmp::Ordering;

use bookie_core::Value;
use bookie_gpui::source::ResultSource;
use tokio_util::sync::CancellationToken;

pub(crate) const MAX_LOCAL_SORT_ROWS: u64 = 100_000;

pub(crate) fn sorted_source_indices(
    source: &dyn ResultSource,
    source_indices: Vec<u32>,
    column: usize,
    ascending: bool,
    cancel: &CancellationToken,
) -> Result<Vec<u32>, &'static str> {
    let mut values = Vec::with_capacity(source_indices.len());
    let mut kind = None;
    for index in source_indices {
        if cancel.is_cancelled() {
            return Err("the operation was cancelled");
        }
        let value = source
            .row_ref(u64::from(index))
            .and_then(|row| row.get(column))
            .ok_or("this result does not expose sortable row values")?;
        if matches!(value, Value::Float(value) if !value.is_finite()) {
            return Err("this column contains non-finite numbers that cannot be sorted safely");
        }
        if !matches!(value, Value::Null) {
            let Some(current_kind) = comparable_kind(value) else {
                return Err("this column contains values that cannot be sorted safely");
            };
            if kind.is_some_and(|previous| previous != current_kind) {
                return Err("this column contains mixed value types and cannot be sorted safely");
            }
            kind = Some(current_kind);
        }
        values.push((index, value));
    }
    values.sort_by(|(_, left), (_, right)| {
        if cancel.is_cancelled() {
            Ordering::Equal
        } else {
            compare_values(left, right, ascending)
        }
    });
    if cancel.is_cancelled() {
        return Err("the operation was cancelled");
    }
    Ok(values.into_iter().map(|(index, _)| index).collect())
}

fn comparable_kind(value: &Value) -> Option<std::mem::Discriminant<Value>> {
    match value {
        Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Text(_)
        | Value::Bytes(_)
        | Value::Date(_)
        | Value::Time(_)
        | Value::DateTime(_)
        | Value::TimestampTz(_)
        | Value::Decimal(_)
        | Value::Uuid(_) => Some(std::mem::discriminant(value)),
        Value::Null | Value::Json(_) | Value::JsonText(_) | Value::Undecodable(_) => None,
    }
}

fn compare_values(left: &Value, right: &Value, ascending: bool) -> Ordering {
    let order = match (left, right) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        _ => compare_non_null(left, right).unwrap_or(Ordering::Equal),
    };
    if ascending || matches!((left, right), (Value::Null, _) | (_, Value::Null)) {
        order
    } else {
        order.reverse()
    }
}

fn compare_non_null(left: &Value, right: &Value) -> Option<Ordering> {
    Some(match (left, right) {
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (Value::Int(left), Value::Int(right)) => left.cmp(right),
        (Value::Float(left), Value::Float(right)) => left.partial_cmp(right).unwrap_or(Ordering::Equal),
        (Value::Text(left), Value::Text(right)) => left.cmp(right),
        (Value::Bytes(left), Value::Bytes(right)) => left.cmp(right),
        (Value::Date(left), Value::Date(right)) => left.cmp(right),
        (Value::Time(left), Value::Time(right)) => left.cmp(right),
        (Value::DateTime(left), Value::DateTime(right)) => left.cmp(right),
        (Value::TimestampTz(left), Value::TimestampTz(right)) => left.cmp(right),
        (Value::Decimal(left), Value::Decimal(right)) => left.cmp(right),
        (Value::Uuid(left), Value::Uuid(right)) => left.cmp(right),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::sorted_source_indices;
    use bookie_core::Value;
    use bookie_gpui::model::{ColumnMeta, Diff, FindResult, Predicate};
    use bookie_gpui::source::ResultSource;
    use rust_decimal::Decimal;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio_util::sync::CancellationToken;

    struct Rows(Vec<Vec<Value>>, AtomicUsize);

    impl ResultSource for Rows {
        fn columns(&self) -> &[ColumnMeta] {
            &[]
        }

        fn total(&self) -> u64 {
            self.0.len() as u64
        }

        fn row(&self, index: u64) -> Vec<Value> {
            self.0.get(index as usize).cloned().unwrap_or_default()
        }

        fn row_ref(&self, index: u64) -> Option<&[Value]> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.get(index as usize).map(Vec::as_slice)
        }

        fn find(&self, _: &str, _: usize, _: &CancellationToken) -> Result<FindResult, &'static str> {
            Err("not used by sort tests")
        }

        fn filter(&self, _: &Predicate, _: &CancellationToken) -> Result<Vec<u32>, &'static str> {
            Err("not used by sort tests")
        }

        fn compare(&self, _: bool, _: &CancellationToken) -> Result<Diff, &'static str> {
            Err("not used by sort tests")
        }
    }

    fn rows(values: Vec<Value>) -> Rows {
        Rows(
            values.into_iter().map(|value| vec![value]).collect(),
            AtomicUsize::new(0),
        )
    }

    #[test]
    fn text_sort_is_stable_null_last_and_reads_values_without_loading_rows() {
        let source = rows(vec![
            Value::Text("pear".into()),
            Value::Null,
            Value::Text("apple".into()),
            Value::Text("apple".into()),
        ]);
        let order = sorted_source_indices(&source, vec![0, 1, 2, 3], 0, true, &CancellationToken::new()).unwrap();
        assert_eq!(order, vec![2, 3, 0, 1]);
        assert_eq!(source.1.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn numeric_sort_is_descending_with_null_last() {
        let source = rows(vec![Value::Int(3), Value::Null, Value::Int(-1), Value::Int(10)]);
        let order = sorted_source_indices(&source, vec![0, 1, 2, 3], 0, false, &CancellationToken::new()).unwrap();
        assert_eq!(order, vec![3, 0, 2, 1]);
    }

    #[test]
    fn decimal_sort_keeps_native_precision() {
        let low = Decimal::from_str_exact("1.0000000000000000001").unwrap();
        let high = Decimal::from_str_exact("1.0000000000000000002").unwrap();
        let source = rows(vec![Value::Decimal(high), Value::Decimal(low)]);
        let order = sorted_source_indices(&source, vec![0, 1], 0, true, &CancellationToken::new()).unwrap();
        assert_eq!(order, vec![1, 0]);
    }

    #[test]
    fn float_sort_keeps_signed_zero_ties_stable() {
        let source = rows(vec![Value::Float(0.0), Value::Float(-0.0), Value::Float(1.0)]);
        let order = sorted_source_indices(&source, vec![0, 1, 2], 0, true, &CancellationToken::new()).unwrap();
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn float_sort_refuses_non_finite_values_with_engine_specific_ordering() {
        for value in [
            Value::Float(f64::NAN),
            Value::Float(f64::INFINITY),
            Value::Float(f64::NEG_INFINITY),
        ] {
            let source = rows(vec![Value::Float(1.0), value]);
            assert!(sorted_source_indices(&source, vec![0, 1], 0, true, &CancellationToken::new()).is_err());
        }
    }

    #[test]
    fn unsupported_and_mixed_types_are_refused_without_lossy_string_ordering() {
        let mixed = rows(vec![Value::Int(1), Value::Text("2".into())]);
        assert!(sorted_source_indices(&mixed, vec![0, 1], 0, true, &CancellationToken::new()).is_err());
        let json = rows(vec![Value::Json(Box::new(serde_json::json!({"a": 1})))]);
        assert!(sorted_source_indices(&json, vec![0], 0, true, &CancellationToken::new()).is_err());
    }

    #[test]
    fn cancelled_sort_and_missing_cells_are_reported() {
        let source = rows(vec![Value::Int(1)]);
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(
            sorted_source_indices(&source, vec![0], 0, true, &cancel),
            Err("the operation was cancelled")
        );
        assert!(sorted_source_indices(&source, vec![0], 1, true, &CancellationToken::new()).is_err());
    }
}
