use std::cmp::Ordering;
use std::collections::HashSet;

use bookie_core::{QueryResult, Value};

pub(super) fn sorted_source_indices(result: &QueryResult, column: usize, ascending: bool) -> Option<Vec<usize>> {
    sorted_source_indices_by_keys(result, &[(column, ascending)])
}

pub(super) fn sorted_source_indices_by_keys(
    result: &QueryResult,
    keys: &[(usize, bool)],
) -> Option<Vec<usize>> {
    if keys.is_empty() {
        return Some((0..result.rows.len()).collect());
    }
    for &(column, _) in keys {
        if !is_sortable_column(result, column) {
            return None;
        }
    }
    let mut order = (0..result.rows.len()).collect::<Vec<_>>();
    for &(column, ascending) in keys.iter().rev() {
        order = sorted_source_indices_for_subset(result, column, ascending, order).ok()?;
    }
    Some(order)
}

pub(super) fn sorted_source_indices_for_subset(
    result: &QueryResult,
    column: usize,
    ascending: bool,
    source_indices: Vec<usize>,
) -> Result<Vec<usize>, Vec<usize>> {
    sorted_source_indices_for_subset_by_keys(result, &[(column, ascending)], source_indices)
}

pub(super) fn sorted_source_indices_for_subset_by_keys(
    result: &QueryResult,
    keys: &[(usize, bool)],
    source_indices: Vec<usize>,
) -> Result<Vec<usize>, Vec<usize>> {
    if keys.is_empty() {
        return Ok(source_indices);
    }
    for &(column, _) in keys {
        if !is_sortable_column(result, column) {
            return Err(source_indices);
        }
        if source_indices
            .iter()
            .any(|source_index| result.rows.get(*source_index).and_then(|row| row.get(column)).is_none())
        {
            return Err(source_indices);
        }
    }
    let mut order = source_indices;
    for &(column, ascending) in keys.iter().rev() {
        order = sort_subset_one_key(result, column, ascending, order);
    }
    Ok(order)
}

fn sort_subset_one_key(
    result: &QueryResult,
    column: usize,
    ascending: bool,
    source_indices: Vec<usize>,
) -> Vec<usize> {
    let sorts_text = source_indices
        .iter()
        .filter_map(|source_index| result.rows.get(*source_index).and_then(|row| row.get(column)))
        .find(|value| !matches!(value, Value::Null))
        .is_some_and(|value| matches!(value, Value::Text(_)));
    if sorts_text {
        return sorted_text_subset(result, column, source_indices, ascending);
    }
    let mut source_indices = source_indices;
    source_indices.sort_by(|left, right| compare_source_values(result, column, left, right, ascending));
    source_indices
}

pub(super) fn is_sortable_column(result: &QueryResult, column: usize) -> bool {
    let mut state = Sortability::new();
    for row in &result.rows {
        let Some(value) = row.get(column) else { return false };
        state.observe(value);
    }
    state.is_sortable
}

pub(super) fn sortable_columns(result: &QueryResult) -> HashSet<usize> {
    let mut states = vec![Sortability::new(); result.columns.len()];
    for row in &result.rows {
        for (state, value) in states.iter_mut().zip(row) {
            state.observe(value);
        }
        for state in states.iter_mut().skip(row.len()) {
            state.is_sortable = false;
        }
    }
    states
        .into_iter()
        .enumerate()
        .filter_map(|(column, state)| state.is_sortable.then_some(column))
        .collect()
}

#[derive(Clone, Copy)]
struct Sortability {
    kind: Option<std::mem::Discriminant<Value>>,
    is_sortable: bool,
}

impl Sortability {
    fn new() -> Self {
        Self {
            kind: None,
            is_sortable: true,
        }
    }

    fn observe(&mut self, value: &Value) {
        if !self.is_sortable || matches!(value, Value::Null) {
            return;
        }
        let Some(kind) = comparable_kind(value) else {
            self.is_sortable = false;
            return;
        };
        if self.kind.is_some_and(|current| current != kind) {
            self.is_sortable = false;
            return;
        }
        self.kind = Some(kind);
    }
}

fn sorted_text_subset(result: &QueryResult, column: usize, source_indices: Vec<usize>, ascending: bool) -> Vec<usize> {
    let mut keyed_indices = source_indices
        .into_iter()
        .map(|source_index| {
            let key = match result.rows.get(source_index).and_then(|row| row.get(column)) {
                Some(Value::Text(text)) => Some(glib::CollationKey::from(text)),
                _ => None,
            };
            (source_index, key)
        })
        .collect::<Vec<_>>();
    keyed_indices.sort_by(|(left, left_key), (right, right_key)| match (left_key, right_key) {
        (Some(left_key), Some(right_key)) => {
            let order = left_key
                .cmp(right_key)
                .then_with(|| compare_source_values(result, column, left, right, true));
            if ascending { order } else { order.reverse() }
        }
        _ => compare_source_values(result, column, left, right, ascending),
    });
    keyed_indices
        .into_iter()
        .map(|(source_index, _)| source_index)
        .collect()
}

fn compare_source_values(
    result: &QueryResult,
    column: usize,
    left: &usize,
    right: &usize,
    ascending: bool,
) -> Ordering {
    match (
        result.rows.get(*left).and_then(|row| row.get(column)),
        result.rows.get(*right).and_then(|row| row.get(column)),
    ) {
        (Some(left), Some(right)) => compare_values(left, right, ascending),
        _ => Ordering::Equal,
    }
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
        (Value::Float(left), Value::Float(right)) => left.total_cmp(right),
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
    use bookie_core::{ColumnInfo, QueryResult, Value};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    use super::{
        sorted_source_indices, sorted_source_indices_by_keys, sorted_source_indices_for_subset,
        sorted_source_indices_for_subset_by_keys,
    };

    fn result(values: Vec<Value>) -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: "value".into(),
                data_type: "".into(),
                nullable: true,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            }],
            rows: values.into_iter().map(|value| vec![value]).collect(),
            truncated: false,
        }
    }

    #[test]
    fn local_sort_orders_native_values_and_keeps_equal_rows_stable() {
        let result = result(vec![Value::Int(10), Value::Int(2), Value::Int(2)]);

        assert_eq!(sorted_source_indices(&result, 0, true), Some(vec![1, 2, 0]));
        assert_eq!(sorted_source_indices(&result, 0, false), Some(vec![0, 1, 2]));
    }

    #[test]
    fn local_sort_reverses_a_single_key() {
        let result = result(vec![Value::Int(1), Value::Int(3), Value::Int(2)]);
        assert_eq!(sorted_source_indices_by_keys(&result, &[(0, true)]), Some(vec![0, 2, 1]));
        assert_eq!(sorted_source_indices_by_keys(&result, &[(0, false)]), Some(vec![1, 2, 0]));
    }

    #[test]
    fn local_sort_applies_secondary_keys_and_clear_restores_source_order() {
        let result = QueryResult {
            columns: vec![
                ColumnInfo {
                    name: "dept".into(),
                    data_type: "".into(),
                    nullable: true,
                    primary_key: false,
                    is_auto_increment: false,
                    default_value: None,
                    is_generated: false,
                    comment: None,
                    collation: None,
                    enum_type: None,
                    domain_type: None,
                },
                ColumnInfo {
                    name: "name".into(),
                    data_type: "".into(),
                    nullable: true,
                    primary_key: false,
                    is_auto_increment: false,
                    default_value: None,
                    is_generated: false,
                    comment: None,
                    collation: None,
                    enum_type: None,
                    domain_type: None,
                },
            ],
            rows: vec![
                vec![Value::Text("b".into()), Value::Text("z".into())],
                vec![Value::Text("a".into()), Value::Text("y".into())],
                vec![Value::Text("a".into()), Value::Text("x".into())],
                vec![Value::Text("b".into()), Value::Text("w".into())],
            ]
            .into(),
            truncated: false,
        };
        assert_eq!(
            sorted_source_indices_by_keys(&result, &[(0, true), (1, true)]),
            Some(vec![2, 1, 3, 0])
        );
        assert_eq!(
            sorted_source_indices_by_keys(&result, &[(0, true), (1, false)]),
            Some(vec![1, 2, 0, 3])
        );
        assert_eq!(
            sorted_source_indices_by_keys(&result, &[]),
            Some(vec![0, 1, 2, 3])
        );
        assert_eq!(
            sorted_source_indices_for_subset_by_keys(&result, &[(0, true), (1, true)], vec![0, 1, 2, 3]),
            Ok(vec![2, 1, 3, 0])
        );
    }

    #[test]
    fn local_sort_keeps_nulls_last_in_both_directions() {
        let result = result(vec![Value::Null, Value::Int(2), Value::Int(1)]);

        assert_eq!(sorted_source_indices(&result, 0, true), Some(vec![2, 1, 0]));
        assert_eq!(sorted_source_indices(&result, 0, false), Some(vec![1, 2, 0]));
    }

    #[test]
    fn local_sort_uses_native_decimal_and_date_ordering() {
        let decimal = result(vec![
            Value::Decimal(Decimal::new(20, 1)),
            Value::Decimal(Decimal::new(3, 1)),
        ]);
        let date = result(vec![
            Value::Date(NaiveDate::from_ymd_opt(2024, 2, 1).unwrap()),
            Value::Date(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
        ]);

        assert_eq!(sorted_source_indices(&decimal, 0, true), Some(vec![1, 0]));
        assert_eq!(sorted_source_indices(&date, 0, true), Some(vec![1, 0]));
    }

    #[test]
    fn local_sort_refuses_mixed_or_undecodable_columns() {
        let mixed = result(vec![Value::Int(1), Value::Text("1".into())]);
        let undecodable = result(vec![Value::Int(1), Value::Undecodable("NUMERIC".into())]);

        assert_eq!(sorted_source_indices(&mixed, 0, true), None);
        assert_eq!(sorted_source_indices(&undecodable, 0, true), None);
    }

    #[test]
    fn subset_sort_only_reorders_the_supplied_source_rows_and_keeps_ties_stable() {
        let result = result(vec![Value::Int(8), Value::Int(2), Value::Int(2), Value::Int(1)]);

        assert_eq!(
            sorted_source_indices_for_subset(&result, 0, true, vec![0, 1, 2]),
            Ok(vec![1, 2, 0])
        );
    }

    #[test]
    fn subset_sort_refuses_a_mixed_column_even_when_the_subset_is_comparable() {
        let result = result(vec![Value::Int(2), Value::Int(1), Value::Text("hidden".into())]);

        assert_eq!(
            sorted_source_indices_for_subset(&result, 0, true, vec![0, 1]),
            Err(vec![0, 1])
        );
    }

    #[test]
    fn subset_sort_keeps_nulls_last_in_descending_order() {
        let result = result(vec![Value::Null, Value::Int(3), Value::Int(1), Value::Int(2)]);

        assert_eq!(
            sorted_source_indices_for_subset(&result, 0, false, vec![0, 1, 2, 3]),
            Ok(vec![1, 3, 2, 0])
        );
    }

    #[test]
    fn text_subset_sort_keeps_nulls_last_and_stable_in_descending_order() {
        let result = result(vec![
            Value::Null,
            Value::Text("apple".into()),
            Value::Null,
            Value::Text("zebra".into()),
        ]);

        assert_eq!(
            sorted_source_indices_for_subset(&result, 0, false, vec![0, 1, 2, 3]),
            Ok(vec![3, 1, 0, 2])
        );
    }

    #[test]
    fn subset_sort_refuses_a_row_missing_the_column_outside_the_subset() {
        let mut result = result(vec![Value::Int(3), Value::Int(1), Value::Int(2)]);
        result.rows.truncate_row(2, 0);

        assert_eq!(
            sorted_source_indices_for_subset(&result, 0, true, vec![0, 1]),
            Err(vec![0, 1])
        );
    }

    #[test]
    fn text_subset_sort_uses_collation_keys() {
        let result = result(vec![
            Value::Text("zebra".into()),
            Value::Text("apple".into()),
            Value::Text("middle".into()),
        ]);

        assert_eq!(
            sorted_source_indices_for_subset(&result, 0, true, vec![0, 1]),
            Ok(vec![1, 0])
        );
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn local_text_sort_follows_the_locale_collation_not_byte_order() {
        let set = unsafe { libc::setlocale(libc::LC_ALL, c"en_US.UTF-8".as_ptr()) };
        if set.is_null() {
            eprintln!("en_US.UTF-8 is not installed; skipping");
            return;
        }
        let result = result(vec![
            Value::Text("Zebra".into()),
            Value::Text("éclair".into()),
            Value::Text("apple".into()),
        ]);
        let order = sorted_source_indices(&result, 0, true).unwrap();
        assert_eq!(order, vec![2, 1, 0], "apple, éclair, Zebra");
        let descending = sorted_source_indices(&result, 0, false).unwrap();
        assert_eq!(descending, vec![0, 1, 2]);
    }
}

#[cfg(test)]
mod duplicate_name_tests {
    use bookie_core::{ColumnInfo, QueryResult, Value};

    use super::sorted_source_indices;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "int".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    #[test]
    fn columns_whose_names_differ_only_in_case_sort_by_their_own_position() {
        let result = QueryResult {
            columns: vec![column("id"), column("ID")],
            rows: vec![
                vec![Value::Int(1), Value::Int(30)],
                vec![Value::Int(2), Value::Int(10)],
                vec![Value::Int(3), Value::Int(20)],
            ]
            .into(),
            truncated: false,
        };
        assert_eq!(sorted_source_indices(&result, 1, true), Some(vec![1, 2, 0]));
        assert_eq!(sorted_source_indices(&result, 0, false), Some(vec![2, 1, 0]));
    }
}
