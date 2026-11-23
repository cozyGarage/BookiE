use bookie_app_services::foreign_key_nav::{ReferencedRowTarget, reference_targets, referencing_targets};
use bookie_core::filter::build_filter_where;
use bookie_core::sql_dialect::qualified_table;
use bookie_core::{TableInfo, Value};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    ToReferenced,
    ToReferencing,
}

impl GridView {
    pub(crate) fn fk_nav_enabled(&self) -> bool {
        self.live.is_some() && !self.grid_is_query && self.compare.is_none() && self.cell_cursor.is_some()
    }

    pub(crate) fn go_to_referenced_row(&mut self, cx: &mut Context<Self>) {
        self.navigate_foreign_key(Direction::ToReferenced, cx);
    }

    pub(crate) fn peek_referenced_row(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone().filter(|_| self.fk_nav_enabled()) else {
            return;
        };
        let Some((row, column)) = self.active_row_with_names() else {
            return;
        };
        let table = self.catalog.grid_table.clone();
        self.status = "Peeking foreign key…".into();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    use bookie_gpui::source::ResultSource;
                    let structure = live.table_structure(&table)?;
                    let mut targets =
                        reference_targets(&structure.foreign_keys, table.schema.as_deref(), &column, &row)?;
                    let target = targets.remove(0);
                    let (sql, params) = target_statement(&live, &target)?;
                    let peek_sql = format!("{sql} LIMIT 1");
                    let cancel = tokio_util::sync::CancellationToken::new();
                    let live_table = live.run_query_bound(&peek_sql, &params, 1, &cancel)?;
                    let cols = live_table.columns();
                    let first_row = live_table.row(0);
                    let fields: Vec<(String, String)> = cols
                        .iter()
                        .enumerate()
                        .map(|(idx, col)| {
                            let val_str = first_row
                                .get(idx)
                                .map_or_else(|| "NULL".to_string(), bookie_gpui::preview::cell_preview);
                            (col.name.clone(), val_str)
                        })
                        .collect();
                    Ok::<_, String>((target.table, fields, sql, params))
                })
                .await;
            this.update(cx, |this, cx| {
                match found {
                    Ok((table_name, fields, sql, params)) => {
                        this.fk_peek = Some(crate::fk_peek_ui::FkPeekState {
                            table: table_name.clone(),
                            fields,
                            sql,
                            params,
                        });
                        this.status = format!("Peeking row in {table_name}");
                    }
                    Err(message) => this.status = message,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn show_referencing_rows(&mut self, cx: &mut Context<Self>) {
        self.navigate_foreign_key(Direction::ToReferencing, cx);
    }

    pub(crate) fn active_row_with_names(&self) -> Option<(Vec<(String, Value)>, String)> {
        let cursor = self.cell_cursor?;
        let names = self.column_names();
        let source = self.source_row(self.base() + cursor.row as u64);
        let cells = self.source.row(source);
        let column = names.get(cursor.column)?.clone();
        Some((names.into_iter().zip(cells).collect(), column))
    }

    fn navigate_foreign_key(&mut self, direction: Direction, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone().filter(|_| self.fk_nav_enabled()) else {
            return;
        };
        let Some((row, column)) = self.active_row_with_names() else {
            return;
        };
        let table = self.catalog.grid_table.clone();
        self.status = "Following the foreign key…".into();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    let structure = live.table_structure(&table)?;
                    let mut targets = match direction {
                        Direction::ToReferenced => {
                            reference_targets(&structure.foreign_keys, table.schema.as_deref(), &column, &row)
                        }
                        Direction::ToReferencing => referencing_targets(&structure.referenced_by, &row),
                    }?;
                    let count = targets.len();
                    let target = targets.remove(0);
                    let statement = target_statement(&live, &target)?;
                    Ok::<_, String>((statement, count, target.table))
                })
                .await;
            this.update(cx, |this, cx| {
                match found {
                    Ok(((sql, params), count, table)) => {
                        this.run_sql_with(sql, params, cx);
                        if count > 1 {
                            this.status = format!("{count} foreign keys match; showing {table}, the first");
                        }
                    }
                    Err(message) => this.status = message,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn target_statement(
    live: &bookie_gpui::live::LiveSession,
    target: &ReferencedRowTarget,
) -> Result<(String, Vec<Value>), String> {
    let table = TableInfo {
        schema: target.schema.clone(),
        name: target.table.clone(),
    };
    let columns = live.table_columns(&table)?;
    let (clause, params) = build_filter_where(&live.driver_id, &columns, &target.filter)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The foreign key gave no condition".to_string())?;
    let qualified = qualified_table(&live.driver_id, target.schema.as_deref(), &target.table);
    Ok((format!("SELECT * FROM {qualified} WHERE {clause}"), params))
}
