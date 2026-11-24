use std::sync::Arc;

use super::{CellCheck, SaveCheck, cell_checks, key_columns, verify};
use crate::model::{Cell, ColumnMeta};
use crate::source::{ResultSource, Source};

struct Rows(Vec<ColumnMeta>, Vec<Vec<Cell>>);

impl ResultSource for Rows {
    fn columns(&self) -> &[ColumnMeta] {
        &self.0
    }
    fn total(&self) -> u64 {
        self.1.len() as u64
    }
    fn row(&self, index: u64) -> Vec<Cell> {
        self.1[index as usize].clone()
    }
    fn find(
        &self,
        _: &str,
        _: usize,
        _: &tokio_util::sync::CancellationToken,
    ) -> Result<crate::model::FindResult, &'static str> {
        Err("unused")
    }
    fn filter(
        &self,
        _: &crate::model::Predicate,
        _: &tokio_util::sync::CancellationToken,
    ) -> Result<Vec<u32>, &'static str> {
        Err("unused")
    }
    fn compare(&self, _: bool, _: &tokio_util::sync::CancellationToken) -> Result<crate::model::Diff, &'static str> {
        Err("unused")
    }
}

fn column(name: &str, primary_key: bool) -> ColumnMeta {
    ColumnMeta {
        name: name.into(),
        width: 100.0,
        numeric: false,
        data_type: String::new(),
        primary_key,
    }
}

fn table(rows: Vec<Vec<Cell>>) -> Source {
    Arc::new(Rows(vec![column("id", true), column("name", false)], rows))
}

fn check(id: i64, name: &str) -> CellCheck {
    CellCheck {
        key: vec![Cell::Int(id)],
        column: 1,
        value: Cell::Text(name.into()),
    }
}

#[test]
fn checks_carry_the_key_of_the_original_row_and_need_a_key() {
    let original = vec![Cell::Int(7), Cell::Text("old".into())];
    let new = Cell::Text("new".into());
    let keys = key_columns(&[column("id", true), column("name", false)]);
    assert_eq!(keys, vec![0]);
    assert_eq!(cell_checks(&keys, [(&original[..], 1, &new)]), vec![check(7, "new")]);
    assert!(cell_checks(&[], [(&original[..], 1, &new)]).is_empty());
}

#[test]
fn a_reread_row_that_matches_is_verified_and_a_changed_one_is_reported() {
    let source = table(vec![
        vec![Cell::Int(1), Cell::Text("kept".into())],
        vec![Cell::Int(2), Cell::Text("TRIGGERED".into())],
    ]);
    let result = verify(source.as_ref(), &[0], &[check(1, "kept"), check(2, "typed")]);
    assert_eq!(result, SaveCheck { matched: 1, differ: 1, unseen: 0 });
    assert!(result.warning().unwrap().starts_with("1 saved cell(s) read back with a different value"));
}

#[test]
fn a_row_missing_from_the_reload_is_counted_as_unchecked_not_as_verified() {
    let source = table(vec![vec![Cell::Int(1), Cell::Text("kept".into())]]);
    let result = verify(source.as_ref(), &[0], &[check(1, "kept"), check(9, "gone")]);
    assert_eq!(result, SaveCheck { matched: 1, differ: 0, unseen: 1 });
    assert!(result.warning().unwrap().contains("not checked"));
    assert_eq!(SaveCheck { matched: 2, differ: 0, unseen: 0 }.warning(), None);
}
