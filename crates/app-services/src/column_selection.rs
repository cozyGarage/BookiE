use std::collections::BTreeSet;

use bookie_core::{ColumnInfo, Value};

use crate::result_copy::{Copied, CopyContext, CopyFormat, render, tsv};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnSelection {
    columns: BTreeSet<usize>,
    anchor: Option<usize>,
}

impl ColumnSelection {
    pub fn select_only(&mut self, column: usize) {
        self.columns.clear();
        self.columns.insert(column);
        self.anchor = Some(column);
    }

    pub fn toggle(&mut self, column: usize) {
        if !self.columns.remove(&column) {
            self.columns.insert(column);
        }
        self.anchor = Some(column);
    }

    pub fn extend_to(&mut self, column: usize) {
        let anchor = self.anchor.unwrap_or(column);
        self.columns = (anchor.min(column)..=anchor.max(column)).collect();
        self.anchor = Some(anchor);
    }

    pub fn clear(&mut self) {
        self.columns.clear();
        self.anchor = None;
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn contains(&self, column: usize) -> bool {
        self.columns.contains(&column)
    }

    pub fn columns(&self) -> Vec<usize> {
        self.columns.iter().copied().collect()
    }

    pub fn retain_below(&mut self, width: usize) {
        self.columns.retain(|column| *column < width);
        if self.columns.is_empty() {
            self.anchor = None;
        }
    }
}

pub fn pick_columns(columns: &[ColumnInfo], selected: &[usize]) -> Vec<ColumnInfo> {
    selected
        .iter()
        .filter_map(|index| columns.get(*index).cloned())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionCopyFormat {
    Tsv,
    Csv,
    Json,
    InList,
}

pub fn copy_selection(
    format: SelectionCopyFormat,
    driver_id: &str,
    columns: &[ColumnInfo],
    rows: &[Vec<Value>],
) -> Result<Copied, String> {
    let copy_format = match format {
        SelectionCopyFormat::Tsv => {
            return Ok(Copied {
                text: tsv(columns, rows.to_vec(), false),
                note: None,
            });
        }
        SelectionCopyFormat::Csv => CopyFormat::Csv,
        SelectionCopyFormat::Json => CopyFormat::Json,
        SelectionCopyFormat::InList => CopyFormat::InList,
    };
    let context = CopyContext {
        columns,
        driver_id,
        target: None,
        column: 0,
    };
    render(copy_format, &context, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
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
    fn selecting_a_column_replaces_the_previous_selection() {
        let mut selection = ColumnSelection::default();
        selection.select_only(2);
        selection.select_only(1);
        assert_eq!(selection.columns(), vec![1]);
        assert!(selection.contains(1) && !selection.contains(2));
    }

    #[test]
    fn toggling_adds_and_removes_a_column() {
        let mut selection = ColumnSelection::default();
        selection.select_only(0);
        selection.toggle(3);
        assert_eq!(selection.columns(), vec![0, 3]);
        selection.toggle(0);
        assert_eq!(selection.columns(), vec![3]);
    }

    #[test]
    fn extending_selects_the_range_from_the_anchor() {
        let mut selection = ColumnSelection::default();
        selection.select_only(4);
        selection.extend_to(2);
        assert_eq!(selection.columns(), vec![2, 3, 4]);
        selection.extend_to(5);
        assert_eq!(selection.columns(), vec![4, 5]);
    }

    #[test]
    fn clearing_and_shrinking_drop_columns() {
        let mut selection = ColumnSelection::default();
        selection.select_only(1);
        selection.toggle(6);
        selection.retain_below(4);
        assert_eq!(selection.columns(), vec![1]);
        selection.clear();
        assert!(selection.is_empty());
    }

    #[test]
    fn picking_columns_keeps_the_selected_infos_in_order() {
        let all = [column("a"), column("b"), column("c")];
        let picked = pick_columns(&all, &[2, 0, 9]);
        let names: Vec<_> = picked.iter().map(|info| info.name.as_str()).collect();
        assert_eq!(names, ["c", "a"]);
    }

    fn rows() -> Vec<Vec<Value>> {
        vec![
            vec![Value::Int(1), Value::Text("x".into())],
            vec![Value::Int(2), Value::Text("it's".into())],
        ]
    }

    #[test]
    fn tsv_copy_has_only_the_selected_projection() {
        let columns = [column("id"), column("name")];
        let copied = copy_selection(SelectionCopyFormat::Tsv, "postgres", &columns, &rows()).unwrap();
        assert_eq!(copied.text.trim_end(), "1\tx\n2\tit's");
    }

    #[test]
    fn csv_and_json_copy_name_the_selected_columns() {
        let columns = [column("id"), column("name")];
        let csv = copy_selection(SelectionCopyFormat::Csv, "postgres", &columns, &rows()).unwrap();
        assert!(csv.text.contains("id,name") && csv.text.contains("2,it's"));
        let json = copy_selection(SelectionCopyFormat::Json, "postgres", &columns, &rows()).unwrap();
        assert!(json.text.contains("\"name\"") && json.text.contains("it's"));
    }

    #[test]
    fn in_list_copy_quotes_literals_for_the_first_column() {
        let columns = [column("name")];
        let rows = vec![vec![Value::Text("it's".into())], vec![Value::Text("b".into())]];
        let copied = copy_selection(SelectionCopyFormat::InList, "postgres", &columns, &rows).unwrap();
        assert_eq!(copied.text, "('it''s', 'b')");
    }

    #[test]
    fn in_list_copy_refuses_when_no_value_is_a_literal() {
        let columns = [column("name")];
        let rows = vec![vec![Value::Null]];
        assert!(copy_selection(SelectionCopyFormat::InList, "postgres", &columns, &rows).is_err());
    }
}
