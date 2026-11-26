use bookie_app_services::foreign_key_nav::{ReferencedRowTarget, reference_targets, referencing_targets};
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
        let (generation, cancel) = self.begin_fk_lookup();
        let table = self.catalog.grid_table.clone();
        let query_table = table.clone();
        let current_session = live.clone();
        self.status = "Peeking foreign key…".into();
        cx.spawn(async move |this, cx| {
            let query_session = live.clone();
            let found = cx
                .background_executor()
                .spawn(async move {
                    use bookie_gpui::source::ResultSource;
                    let structure = query_session.table_structure(&query_table)?;
                    if cancel.is_cancelled() {
                        return Err("Canceled the foreign key peek".into());
                    }
                    let mut targets =
                        reference_targets(&structure.foreign_keys, query_table.schema.as_deref(), &column, &row)?;
                    let target = targets.remove(0);
                    let (sql, params) = target_statement(&query_session, &target)?;
                    if cancel.is_cancelled() {
                        return Err("Canceled the foreign key peek".into());
                    }
                    let live_table = query_session.run_query_bound(&sql, &params, 1, &cancel)?;
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
                if !this.fk_lookup_is_current(generation, &current_session, &table) {
                    return;
                }
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

    pub(crate) fn clear_fk_peek(&mut self) {
        self.fk_peek_work.invalidate();
        self.fk_peek = None;
    }

    fn begin_fk_lookup(&mut self) -> (u64, CancellationToken) {
        self.clear_fk_peek();
        let cancel = CancellationToken::new();
        self.fk_peek_work.cancel = cancel.clone();
        (self.fk_peek_work.generation, cancel)
    }

    pub(crate) fn fk_lookup_is_current(
        &self,
        generation: u64,
        session: &std::sync::Arc<bookie_gpui::live::LiveSession>,
        table: &TableInfo,
    ) -> bool {
        self.fk_peek_work.generation == generation
            && self.is_current_session(session)
            && self.catalog.grid_table == *table
            && matches!(self.tabs.current(), Tab::Table(active) if active == table)
            && !self.grid_is_query
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
        let (generation, cancel) = self.begin_fk_lookup();
        let table = self.catalog.grid_table.clone();
        let query_table = table.clone();
        let query_session = live.clone();
        self.status = "Following the foreign key…".into();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    if cancel.is_cancelled() {
                        return Err("Canceled the foreign key lookup".into());
                    }
                    let structure = query_session.table_structure(&query_table)?;
                    if cancel.is_cancelled() {
                        return Err("Canceled the foreign key lookup".into());
                    }
                    let mut targets = match direction {
                        Direction::ToReferenced => {
                            reference_targets(&structure.foreign_keys, query_table.schema.as_deref(), &column, &row)
                        }
                        Direction::ToReferencing => referencing_targets(&structure.referenced_by, &row),
                    }?;
                    let count = targets.len();
                    let target = targets.remove(0);
                    let statement = target_statement(&query_session, &target)?;
                    Ok::<_, String>((statement, count, target.table))
                })
                .await;
            this.update(cx, |this, cx| {
                if !this.fk_lookup_is_current(generation, &live, &table) {
                    return;
                }
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
    let statements = target.statements(&live.driver_id, &columns)?;
    Ok((statements.select, statements.params))
}
