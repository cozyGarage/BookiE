use bookie_app_services::selection_stats::StatsAccumulator;
pub(in crate::ui::browse_tab) use bookie_app_services::selection_stats::{ColumnStats, tooltip};
use bookie_core::ColumnInfo;

pub(in crate::ui::browse_tab) fn column_stats(
    columns: &[ColumnInfo],
    rows: &[crate::ui::row_object::RowObject],
) -> Vec<ColumnStats> {
    column_stats_where(columns, rows, |_| true)
}

pub(in crate::ui::browse_tab) fn column_stats_in(
    columns: &[ColumnInfo],
    rows: &[crate::ui::row_object::RowObject],
    only: &[usize],
) -> Vec<ColumnStats> {
    column_stats_where(columns, rows, |index| only.contains(&index))
}

fn column_stats_where(
    columns: &[ColumnInfo],
    rows: &[crate::ui::row_object::RowObject],
    wanted: impl Fn(usize) -> bool,
) -> Vec<ColumnStats> {
    columns
        .iter()
        .enumerate()
        .filter(|(index, _)| wanted(*index))
        .filter_map(|(index, column)| {
            let mut accumulator = StatsAccumulator::default();
            for row in rows {
                if row.with_cell_value(index, |value| accumulator.push(value)).is_none() {
                    accumulator.unreadable();
                }
            }
            accumulator.finish(&column.name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bookie_core::Value;

    use super::*;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: String::new(),
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
    fn unfetched_cells_omit_only_the_affected_column() {
        let rows = [crate::ui::row_object::RowObject::new_projected(vec![Value::Int(7)], &[0], 2).unwrap()];
        let stats = column_stats(&[column("fetched"), column("unfetched")], &rows);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].column, "fetched");
        assert_eq!(stats[0].sum, "7");
    }

    #[test]
    fn rows_are_summarised_through_the_shared_accumulator() {
        let rows = [3, 4].map(|n| crate::ui::row_object::RowObject::new(vec![Value::Int(n)]));
        let stats = column_stats_in(&[column("n")], &rows, &[0]);
        assert_eq!(stats[0].sum, "7");
    }
}
