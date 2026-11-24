use bookie_app_services::foreign_key_nav::{ReferencedRowTarget, referencing_targets};
use bookie_app_services::result_copy::{self, CopyContext, CopyFormat};
use bookie_core::filter::build_filter_where;
use bookie_core::sql_dialect::qualified_table;
use bookie_core::{ForeignKeyInfo, InboundForeignKeyInfo, TableInfo, Value};
use gpui::Context;
use tokio_util::sync::CancellationToken;

use crate::{GridView, Ticket};

pub(crate) const REFERENCE_COUNT_CAP: usize = 1_000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TableKeys {
    pub table: TableInfo,
    pub foreign_key_columns: Vec<String>,
    pub referenced_by: Vec<InboundForeignKeyInfo>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReferenceCount {
    pub label: String,
    pub count: Result<(usize, bool), String>,
    pub sql: String,
    pub params: Vec<Value>,
}

#[derive(Default)]
pub(crate) struct RowInspector {
    pub json: bool,
    pub keys: Option<TableKeys>,
    keys_requested: Option<TableInfo>,
    pub references: Vec<ReferenceCount>,
    references_for: Option<(TableInfo, u64)>,
    work: Ticket,
}

impl GridView {
    pub(crate) fn ensure_row_inspector(&mut self, cx: &mut Context<Self>) {
        self.ensure_table_keys(cx);
        self.ensure_row_references(cx);
    }

    fn ensure_table_keys(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else {
            return;
        };
        let table = self.catalog.grid_table.clone();
        if self.grid_is_query || table.name.is_empty() || self.row_inspector.keys_requested.as_ref() == Some(&table) {
            return;
        }
        self.row_inspector.keys_requested = Some(table.clone());
        self.row_inspector.keys = None;
        let supports_keys = drivers_registry::default_registry()
            .get(&live.driver_id)
            .is_some_and(|driver| driver.capabilities().supports_foreign_key_metadata);
        if !supports_keys {
            return;
        }
        cx.spawn(async move |this, cx| {
            let query_session = live.clone();
            let query_table = table.clone();
            let structure = cx
                .background_executor()
                .spawn(async move { query_session.table_structure(&query_table) })
                .await;
            this.update(cx, |this, cx| {
                if !this.is_current_session(&live) || this.catalog.grid_table != table {
                    return;
                }
                if let Ok(structure) = structure {
                    this.row_inspector.keys = Some(TableKeys {
                        table,
                        foreign_key_columns: foreign_key_columns(&structure.foreign_keys),
                        referenced_by: structure.referenced_by,
                    });
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn ensure_row_references(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else {
            return;
        };
        let Some(keys) = self
            .row_inspector
            .keys
            .clone()
            .filter(|keys| keys.table == self.catalog.grid_table)
        else {
            return;
        };
        if !self.row_detail_open || self.grid_is_query || keys.referenced_by.is_empty() {
            return;
        }
        let Some(cursor) = self.cell_cursor else {
            return;
        };
        let source_row = self.source_row(self.base() + cursor.row as u64);
        let wanted = (keys.table.clone(), source_row);
        if self.row_inspector.references_for.as_ref() == Some(&wanted) {
            return;
        }
        self.row_inspector.references_for = Some(wanted);
        self.row_inspector.references.clear();
        self.row_inspector.work.invalidate();
        let generation = self.row_inspector.work.generation;
        let cancel = CancellationToken::new();
        self.row_inspector.work.cancel = cancel.clone();
        let names = self.column_names();
        let row: Vec<(String, Value)> = names.into_iter().zip(self.source.row(source_row)).collect();
        cx.spawn(async move |this, cx| {
            let query_session = live.clone();
            let counted = cx
                .background_executor()
                .spawn(async move { count_references(&query_session, &keys.referenced_by, &row, &cancel) })
                .await;
            this.update(cx, |this, cx| {
                if this.row_inspector.work.generation == generation && this.is_current_session(&live) {
                    this.row_inspector.references = counted;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn open_reference(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(reference) = self.row_inspector.references.get(index).cloned() else {
            return;
        };
        self.run_sql_with(reference.sql, reference.params, cx);
    }

    pub(crate) fn row_detail_json_text(&self) -> Option<String> {
        let cursor = self.cell_cursor?;
        let source_row = self.source_row(self.base() + cursor.row as u64);
        let mut row = self.source.row(source_row);
        for (column, value) in row.iter_mut().enumerate() {
            if let Some(edited) = self.cell_edit.edits.get(&(source_row, column)) {
                *value = edited.clone();
            }
        }
        let columns = bookie_gpui::core_bridge::column_infos_for(self.source.columns());
        let driver_id = self.live.as_ref().map_or("postgres", |live| live.driver_id.as_str());
        let context = CopyContext {
            columns: &columns,
            driver_id,
            target: None,
            column: 0,
        };
        let copied = result_copy::render(CopyFormat::Json, &context, &[row]).ok()?;
        Some(pretty_single_row(&copied.text))
    }

    pub(crate) fn follows_link(&self, column: usize, modifiers: gpui::Modifiers) -> bool {
        let control = modifiers.control || (cfg!(target_os = "macos") && modifiers.platform);
        control && !modifiers.shift && !modifiers.alt && self.link_columns().get(column).copied().unwrap_or(false)
    }

    pub(crate) fn link_columns(&self) -> Vec<bool> {
        let Some(keys) = self
            .row_inspector
            .keys
            .as_ref()
            .filter(|keys| keys.table == self.catalog.grid_table)
        else {
            return Vec::new();
        };
        if self.grid_is_query || self.compare.is_some() {
            return Vec::new();
        }
        self.source
            .columns()
            .iter()
            .map(|column| keys.foreign_key_columns.contains(&column.name))
            .collect()
    }
}

pub(crate) fn foreign_key_columns(keys: &[ForeignKeyInfo]) -> Vec<String> {
    let mut columns: Vec<String> = keys.iter().flat_map(|key| key.columns.iter().cloned()).collect();
    columns.sort();
    columns.dedup();
    columns
}

fn count_references(
    live: &std::sync::Arc<bookie_gpui::live::LiveSession>,
    inbound: &[InboundForeignKeyInfo],
    row: &[(String, Value)],
    cancel: &CancellationToken,
) -> Vec<ReferenceCount> {
    let targets = match referencing_targets(inbound, row) {
        Ok(targets) => targets,
        Err(message) => {
            return vec![ReferenceCount {
                label: "Referenced by".into(),
                count: Err(message),
                sql: String::new(),
                params: Vec::new(),
            }];
        }
    };
    targets
        .into_iter()
        .take_while(|_| !cancel.is_cancelled())
        .map(|target| {
            let label = format!("{} ({})", target.table, target.foreign_key);
            match reference_statements(live, &target) {
                Ok(((sql, params), (probe, probe_params))) => {
                    let count = live
                        .run_query_bound(&probe, &probe_params, REFERENCE_COUNT_CAP as u64 + 1, cancel)
                        .map(|table| {
                            let rows =
                                usize::try_from(bookie_gpui::source::ResultSource::total(&table)).unwrap_or(usize::MAX);
                            (
                                rows.min(REFERENCE_COUNT_CAP),
                                rows > REFERENCE_COUNT_CAP || table.truncated,
                            )
                        });
                    ReferenceCount {
                        label,
                        count,
                        sql,
                        params,
                    }
                }
                Err(message) => ReferenceCount {
                    label,
                    count: Err(message),
                    sql: String::new(),
                    params: Vec::new(),
                },
            }
        })
        .collect()
}

type Statement = (String, Vec<Value>);

fn reference_statements(
    live: &bookie_gpui::live::LiveSession,
    target: &ReferencedRowTarget,
) -> Result<(Statement, Statement), String> {
    let table = TableInfo {
        schema: target.schema.clone(),
        name: target.table.clone(),
    };
    let columns = live.table_columns(&table)?;
    let (clause, params) = build_filter_where(&live.driver_id, &columns, &target.filter)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The foreign key gave no condition".to_string())?;
    let qualified = qualified_table(&live.driver_id, target.schema.as_deref(), &target.table);
    Ok((
        (format!("SELECT * FROM {qualified} WHERE {clause}"), params.clone()),
        (format!("SELECT 1 FROM {qualified} WHERE {clause}"), params),
    ))
}

pub(crate) fn pretty_single_row(json: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Array(mut rows)) if rows.len() == 1 => {
            serde_json::to_string_pretty(&rows.remove(0)).unwrap_or_else(|_| json.to_string())
        }
        Ok(other) => serde_json::to_string_pretty(&other).unwrap_or_else(|_| json.to_string()),
        Err(_) => json.to_string(),
    }
}

pub(crate) fn count_label(count: &Result<(usize, bool), String>) -> String {
    match count {
        Ok((rows, true)) => format!("{rows}+ rows"),
        Ok((1, false)) => "1 row".into(),
        Ok((rows, false)) => format!("{rows} rows"),
        Err(message) => message.clone(),
    }
}
