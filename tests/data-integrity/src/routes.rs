use std::collections::BTreeMap;
use std::path::Path;

use bookie_app_services::cell_save_plan::{CellChange, CellSavePlan, SaveTarget, plan_cell_saves, plan_row_insert};
use bookie_app_services::clipboard_grid::parse_clipboard_grid;
use bookie_app_services::csv_import_plan::{ImportChoices, plan_csv_import};
use bookie_app_services::result_copy;
use bookie_app_services::set_cells::{SetCellTarget, SetCellsRequest, SetCellsValue, plan_set_cells};
use bookie_app_services::value_display::{copy_text, format_temporal_for_edit};
use bookie_app_services::value_input::{ParseHooks, parse_grid_cell_input_with_style, parse_pasted_cell_with_style};
use bookie_core::export::{
    CsvOptions, ResultExport, ResultFormat, SqlTarget, unique_csv_null_marker, write_result_file,
};
use bookie_core::{ColumnInfo, Connection, QueryResult, Value};

use crate::artefact::{self, Cells};
use crate::corpus::{Case, Corpus, Route};
use crate::governed::{self, Governed, control};
use crate::report::{CaseResult, Outcome};
use crate::rules::{Carrier, RefereeColumn, Rule};
use crate::runner::{self, RefereeRow};
use crate::server::{Dialect, Server};

const IMPORT_ROW_CAP: usize = 100_000;

fn offset(route: Route) -> i64 {
    match route {
        Route::Edit => 1_000,
        Route::Paste => 2_000,
        Route::SetCells => 3_000,
        _ => 4_000,
    }
}

fn clone_table(case: &Case, route: Route) -> String {
    format!("{}_{}", case.table(), route.name())
}

pub struct Prepared<'a> {
    case: &'a Case,
    before: Vec<RefereeRow>,
}

const REFUSED: &str = "refused: ";

#[derive(Default)]
struct Written {
    ids: Vec<i64>,
    refused: Vec<String>,
}

enum Work {
    Cells(Cells),
    Written(Result<Written, String>),
}

fn all_written(ids: &[i64]) -> Written {
    Written {
        ids: ids.to_vec(),
        refused: Vec::new(),
    }
}

pub async fn run_routes(server: &Server, corpus: &Corpus, edits: bool) -> Vec<CaseResult> {
    let mut prepared = Vec::new();
    let mut results = Vec::new();
    for case in corpus.cases.iter().filter(|case| case.applies_to(&server.label)) {
        match prepare(server, case, edits).await {
            Ok(before) => prepared.push(Prepared { case, before }),
            Err(error) => results.extend(broken(server, case, &error)),
        }
    }
    let governed = governed::open_writable(server.dialect.driver_id, server.options.clone()).await;
    let mut work = Vec::new();
    for item in &prepared {
        work.push(perform(&governed, server.dialect, item.case, &item.before, edits).await);
    }
    governed.close().await;
    for (item, routes) in prepared.iter().zip(work) {
        results.extend(judge(server, item, routes).await);
    }
    results
}

fn routes(edits: bool) -> Vec<Route> {
    let mut routes = Route::ARTEFACTS.to_vec();
    routes.extend(Route::COPIES);
    if edits {
        routes.extend(Route::EDITS);
        routes.push(Route::NoopEdit);
    }
    routes
}

fn broken(server: &Server, case: &Case, error: &str) -> Vec<CaseResult> {
    routes(true)
        .into_iter()
        .map(|route| CaseResult {
            server: server.label.clone(),
            case: case.id.clone(),
            route,
            outcome: Outcome::Broken(error.to_string()),
        })
        .collect()
}

async fn prepare(server: &Server, case: &Case, edits: bool) -> Result<Vec<RefereeRow>, String> {
    runner::prepare_case(server, case).await?;
    for route in Route::COPIES {
        let clone = clone_table(case, route);
        server
            .run_sql(&Dialect::fill(server.dialect.drop_table, &clone, ""))
            .await?;
        runner::create_table(server, case, &clone).await?;
    }
    let before = runner::referee_rows(server, case).await?;
    if edits {
        let table = case.table();
        for route in [Route::Edit, Route::Paste, Route::SetCells] {
            for row in &before {
                let id = row.id + offset(route);
                server
                    .run_sql(&format!("INSERT INTO {table} (id, v) VALUES ({id}, NULL)"))
                    .await
                    .map_err(|error| format!("placeholder row {id}: {error}"))?;
            }
        }
    }
    Ok(before)
}

async fn perform(
    governed: &Governed,
    dialect: &Dialect,
    case: &Case,
    before: &[RefereeRow],
    edits: bool,
) -> BTreeMap<Route, Work> {
    let mut work = BTreeMap::new();
    let table = case.table();
    let ids: Vec<i64> = before.iter().map(|row| row.id).collect();
    let result = match read_original(governed, &table, &ids).await {
        Ok(result) => result,
        Err(error) => {
            for route in routes(edits) {
                work.insert(route, Work::Written(Err(error.clone())));
            }
            return work;
        }
    };
    let directory = tempfile::TempDir::new().expect("export directory");
    work.insert(Route::CopyTsv, Work::Cells(copy_tsv(&result)));
    let keep_raw = matches!(case.carrier, Carrier::Json | Carrier::JsonText);
    for route in [Route::ExportCsv, Route::ExportJson, Route::ExportJsonLines] {
        work.insert(
            route,
            Work::Cells(export_cells(route, &result, directory.path(), keep_raw)),
        );
    }
    for route in Route::COPIES {
        let copied = copy_into_clone(governed, dialect, case, route, &result, directory.path()).await;
        work.insert(route, Work::Written(copied.map(|()| all_written(&ids))));
    }
    if edits {
        let columns = match columns_of(governed, dialect, &table).await {
            Ok(columns) => columns,
            Err(error) => return fail_edits(work, &error),
        };
        for route in Route::EDITS.into_iter().chain([Route::NoopEdit]) {
            let written = edit_rows(governed, dialect, &table, &columns, route, &result).await;
            work.insert(route, Work::Written(written));
        }
    }
    work
}

fn fail_edits(mut work: BTreeMap<Route, Work>, error: &str) -> BTreeMap<Route, Work> {
    for route in Route::EDITS.into_iter().chain([Route::NoopEdit]) {
        work.insert(route, Work::Written(Err(error.to_string())));
    }
    work
}

async fn read_original(governed: &Governed, table: &str, ids: &[i64]) -> Result<QueryResult, String> {
    let sql = format!("SELECT id, v FROM {table} ORDER BY id");
    let batch = governed
        .guard
        .query_result_sets_controlled(&sql, &[], &control())
        .await
        .map_err(|error| format!("query failed: {error}"))?;
    let mut result = batch.result_sets.into_iter().next().ok_or("no result set")?;
    let rows: Vec<Vec<Value>> = result
        .rows
        .iter()
        .filter(|row| matches!(row.first(), Some(Value::Int(id)) if ids.contains(id)))
        .map(<[Value]>::to_vec)
        .collect();
    result.rows = rows.into();
    Ok(result)
}

async fn columns_of(governed: &Governed, dialect: &Dialect, table: &str) -> Result<Vec<ColumnInfo>, String> {
    governed
        .guard
        .fetch_columns_controlled(dialect.schema, table, &control())
        .await
        .map_err(|error| format!("columns of {table}: {error}"))
}

fn copy_tsv(result: &QueryResult) -> Cells {
    let text = result_copy::tsv(&result.columns, &result.rows, false);
    let mut cells = BTreeMap::new();
    for row in parse_clipboard_grid(&text) {
        let id = row
            .first()
            .cloned()
            .flatten()
            .and_then(|id| id.parse().ok())
            .ok_or_else(|| format!("pasted row without an id: {row:?}"))?;
        cells.insert(id, row.get(1).cloned().flatten());
    }
    Ok(cells)
}

fn raw_csv(result: &QueryResult) -> CsvOptions {
    CsvOptions {
        null_to_empty: false,
        null_marker: Some(unique_csv_null_marker(&result.rows)),
        sanitize_formulas: false,
        ..CsvOptions::default()
    }
}

fn export(path: &Path, result: &QueryResult, format: ResultFormat, sql: Option<SqlTarget<'_>>) -> Result<(), String> {
    let csv = raw_csv(result);
    let export = ResultExport { format, csv: &csv, sql };
    write_result_file(path, result, &export, || false, |_| {})
        .map_err(|error| format!("{REFUSED}{format:?} export: {error}"))
}

fn export_cells(route: Route, result: &QueryResult, directory: &Path, keep_raw: bool) -> Cells {
    let path = directory.join(route.name());
    let format = match route {
        Route::ExportCsv => ResultFormat::Csv,
        Route::ExportJson => ResultFormat::Json,
        _ => ResultFormat::JsonLines,
    };
    export(&path, result, format, None)?;
    let text = std::fs::read_to_string(&path).map_err(|error| format!("read artefact: {error}"))?;
    match route {
        Route::ExportCsv => {
            let marker = raw_csv(result).null_marker;
            artefact::delimited_cells(&text, b',', true, marker.as_deref())
        }
        Route::ExportJson => {
            let rows: Vec<artefact::RawRow> =
                serde_json::from_str(&text).map_err(|error| format!("JSON artefact does not parse: {error}"))?;
            artefact::json_cells(&rows, keep_raw)
        }
        _ => {
            let rows = text
                .lines()
                .map(serde_json::from_str)
                .collect::<Result<Vec<artefact::RawRow>, _>>()
                .map_err(|error| format!("JSON Lines artefact does not parse: {error}"))?;
            artefact::json_cells(&rows, keep_raw)
        }
    }
}

async fn copy_into_clone(
    governed: &Governed,
    dialect: &Dialect,
    case: &Case,
    route: Route,
    result: &QueryResult,
    directory: &Path,
) -> Result<(), String> {
    let clone = clone_table(case, route);
    let path = directory.join(format!("{}.artefact", route.name()));
    let statements = match route {
        Route::ExportSql => {
            let target = SqlTarget {
                driver_id: dialect.driver_id,
                schema: dialect.schema,
                table: &clone,
            };
            export(&path, result, ResultFormat::Sql, Some(target))?;
            let script = std::fs::read_to_string(&path).map_err(|error| format!("read SQL artefact: {error}"))?;
            bookie_core::sql_lex::split_statements(&script, dialect.driver_id)
                .into_iter()
                .map(|statement| (statement, Vec::new()))
                .collect()
        }
        _ => import_statements(governed, dialect, route, result, &clone, &path).await?,
    };
    apply(governed, &statements, &[]).await
}

async fn import_statements(
    governed: &Governed,
    dialect: &Dialect,
    route: Route,
    result: &QueryResult,
    clone: &str,
    path: &Path,
) -> Result<Vec<(String, Vec<Value>)>, String> {
    let (format, null_marker) = match route {
        Route::ImportXlsx => (ResultFormat::Xlsx, String::new()),
        _ => (ResultFormat::Csv, raw_csv(result).null_marker.unwrap_or_default()),
    };
    export(path, result, format, None)?;
    let columns = columns_of(governed, dialect, clone).await?;
    let target = SaveTarget {
        driver_id: dialect.driver_id,
        schema: dialect.schema,
        table: clone,
        columns: &columns,
    };
    let choices = ImportChoices {
        delimiter: None,
        null_marker,
    };
    plan_csv_import(target, governed.capabilities, path, IMPORT_ROW_CAP, &choices)
        .map(|plan| { println!("DIAGNOSTIC {route:?} {:?}", plan.statements); plan.statements })
        .map_err(|error| format!("{REFUSED}import plan: {error}"))
}

async fn apply(governed: &Governed, statements: &[(String, Vec<Value>)], expect_one: &[usize]) -> Result<(), String> {
    if statements.is_empty() {
        return Ok(());
    }
    governed
        .guard
        .execute_in_transaction_checked_controlled(statements, expect_one, &control())
        .await
        .map(|_| ())
        .map_err(|error| format!("{REFUSED}save: {error}"))
}

fn editable(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bytes(_) | Value::Undecodable(_) => false,
        Value::Text(text) => !text.contains('\0'),
        _ => true,
    }
}

fn edit_text(value: &Value) -> String {
    format_temporal_for_edit(value).unwrap_or_else(|| copy_text(value).into_string())
}

async fn edit_rows(
    governed: &Governed,
    dialect: &Dialect,
    table: &str,
    columns: &[ColumnInfo],
    route: Route,
    result: &QueryResult,
) -> Result<Written, String> {
    let hooks = ParseHooks::new(governed.capabilities.enum_input_style, governed.value_input);
    let target = SaveTarget {
        driver_id: dialect.driver_id,
        schema: dialect.schema,
        table,
        columns,
    };
    let names: Vec<String> = columns.iter().map(|column| column.name.clone()).collect();
    let value_column = names
        .iter()
        .position(|name| name == "v")
        .ok_or("table has no v column")?;
    let mut written = Written::default();
    for row in result.rows.iter() {
        let (Some(Value::Int(id)), Some(value)) = (row.first(), row.get(1)) else {
            continue;
        };
        if !editable(value) || (route != Route::Paste && edit_text(value).is_empty()) {
            continue;
        }
        let edit = RowEdit {
            id: *id,
            value,
            column: &columns[value_column],
            value_column,
            names: &names,
            hooks: &hooks,
        };
        let planned = plan_edit(target, route, &edit);
        let outcome = match planned {
            Ok(plan) => apply(governed, &plan.statements, &plan.expect_one).await,
            Err(error) => Err(error),
        };
        match outcome {
            Ok(()) => written.ids.push(*id),
            Err(error) => written
                .refused
                .push(format!("id {id}: {}", error.trim_start_matches(REFUSED))),
        }
    }
    Ok(written)
}

struct RowEdit<'a> {
    id: i64,
    value: &'a Value,
    column: &'a ColumnInfo,
    value_column: usize,
    names: &'a [String],
    hooks: &'a ParseHooks,
}

fn row_of(edit: &RowEdit<'_>, id: i64, value: Value) -> Vec<Value> {
    let mut row = vec![Value::Null; edit.names.len()];
    if let Some(at) = edit.names.iter().position(|name| name == "id") {
        row[at] = Value::Int(id);
    }
    row[edit.value_column] = value;
    row
}

fn plan_edit(target: SaveTarget<'_>, route: Route, edit: &RowEdit<'_>) -> Result<CellSavePlan, String> {
    let text = edit_text(edit.value);
    let new_id = edit.id + offset(route);
    let parsed = match route {
        Route::InsertRow => {
            let id_column = edit.names.iter().position(|name| name == "id").ok_or("no id column")?;
            let values = [(id_column, new_id.to_string()), (edit.value_column, text)];
            return plan_row_insert(target, &values, edit.hooks).map_err(|error| format!("insert plan: {error}"));
        }
        Route::SetCells => return plan_set_cell(target, edit, &text, new_id),
        Route::Paste => {
            let copied = copy_text(edit.value).into_string();
            parse_pasted_cell_with_style(Some(&copied), Some(edit.column), edit.hooks, Some(&Value::Null))
        }
        Route::NoopEdit => parse_grid_cell_input_with_style(&text, Some(edit.column), edit.hooks, Some(edit.value)),
        _ => parse_grid_cell_input_with_style(&text, Some(edit.column), edit.hooks, Some(&Value::Null)),
    }
    .map_err(|error| format!("input {text:?}: {error:?}"))?;
    let (key, original) = match route {
        Route::NoopEdit => (edit.id, edit.value.clone()),
        _ => (new_id, Value::Null),
    };
    let change = CellChange {
        original_row: row_of(edit, key, original),
        column: edit.value_column,
        new_value: parsed,
    };
    plan_cell_saves(target, edit.names, &[change]).map_err(|error| format!("save plan: {error}"))
}

fn plan_set_cell(target: SaveTarget<'_>, edit: &RowEdit<'_>, text: &str, new_id: i64) -> Result<CellSavePlan, String> {
    let value = SetCellsValue::Text(text.to_string());
    let targets = [SetCellTarget {
        position: 0,
        column: edit.value_column,
        row_key: Some(vec![Value::Int(new_id)]),
        original: Value::Null,
    }];
    let plan = plan_set_cells(SetCellsRequest {
        columns: target.columns,
        table_editable: true,
        value: &value,
        targets: &targets,
        hooks: edit.hooks,
        cell_allows_edit: |_, value| !matches!(value, Value::Undecodable(_)),
    })
    .map_err(|error| format!("set cells: {error:?}"))?;
    let changes: Vec<CellChange> = plan
        .edits
        .into_iter()
        .map(|planned| CellChange {
            original_row: row_of(edit, new_id, planned.original),
            column: planned.column,
            new_value: planned.value,
        })
        .collect();
    plan_cell_saves(target, edit.names, &changes).map_err(|error| format!("save plan: {error}"))
}

async fn judge(server: &Server, item: &Prepared<'_>, mut work: BTreeMap<Route, Work>) -> Vec<CaseResult> {
    let after = runner::referee_rows(server, item.case).await;
    let mut results = Vec::new();
    for route in routes(work.contains_key(&Route::Edit)) {
        let outcome = match (case_skip(item.case, route), work.remove(&route)) {
            (Some(reason), _) => Outcome::Skipped(reason),
            (None, Some(Work::Cells(cells))) => compare_cells(server, item.case, &item.before, cells).await,
            (None, Some(Work::Written(written))) => compare_written(server, item, route, written, after.clone()).await,
            (None, None) => continue,
        };
        results.push(CaseResult {
            server: server.label.clone(),
            case: item.case.id.clone(),
            route,
            outcome,
        });
    }
    results
}

fn case_skip(case: &Case, route: Route) -> Option<String> {
    case.skip_reason(route).map(str::to_string)
}

async fn compare_cells(server_handle: &Server, case: &Case, before: &[RefereeRow], cells: Cells) -> Outcome {
    let cells = match cells {
        Ok(cells) => cells,
        Err(error) => return refused_or_failed(error),
    };
    let mut failures = Vec::new();
    for row in before {
        let expected = case.expectation(Route::Query, row.type_name.as_deref());
        let server = row.columns.get(&expected.rule.referee_column()).cloned().flatten();
        let Some(text) = cells.get(&row.id) else {
            failures.push(format!("id {}: row missing from the artefact", row.id));
            continue;
        };
        let text = match text.as_deref().map(artefact::sqlite_any_payload) {
            Some(Some(payload)) => Some(payload),
            _ => text.clone(),
        };
        let unparsed = text
            .as_deref()
            .is_some_and(|text| artefact::value_from_text(expected.carrier, text).is_none());
        let text = match (&text, expected.rule) {
            (Some(text), rule) if rule == Rule::ServerCast || unparsed => {
                match runner::server_render(server_handle, case, text).await {
                    Ok(rendered) => Some(rendered),
                    Err(error) => {
                        failures.push(format!("id {}: {error}", row.id));
                        continue;
                    }
                }
            }
            _ => text,
        };
        let server_text = row.columns.get(&RefereeColumn::Text).cloned().flatten();
        if let Err(error) = artefact::check_text(
            expected.rule,
            expected.carrier,
            text.as_deref(),
            server.as_deref(),
            server_text.as_deref(),
        ) {
            failures.push(format!("id {}: {error}", row.id));
        }
    }
    if failures.is_empty() {
        Outcome::Pass { rows: before.len() }
    } else {
        Outcome::Fail(failures)
    }
}

async fn compare_written(
    server: &Server,
    item: &Prepared<'_>,
    route: Route,
    written: Result<Written, String>,
    after: Result<Vec<RefereeRow>, String>,
) -> Outcome {
    let written = match written {
        Ok(written) => written,
        Err(error) => return refused_or_failed(error),
    };
    let target = match route {
        Route::ExportSql | Route::ImportCsv | Route::ImportXlsx => {
            runner::referee_rows_of(server, item.case, &clone_table(item.case, route)).await
        }
        _ => after,
    };
    let target = match target {
        Ok(rows) => rows,
        Err(error) => return Outcome::Broken(format!("referee failed: {error}")),
    };
    if written.ids.is_empty() && written.refused.is_empty() && route != Route::NoopEdit {
        return Outcome::Skipped("no row holds a value the grid can edit inline".into());
    }
    let shift = match route {
        Route::Edit | Route::Paste | Route::SetCells | Route::InsertRow => offset(route),
        _ => 0,
    };
    let rows: Vec<&RefereeRow> = match route {
        Route::NoopEdit => item.before.iter().collect(),
        _ => item.before.iter().filter(|row| written.ids.contains(&row.id)).collect(),
    };
    let failures = same_rows(&rows, &target, shift);
    match (failures.is_empty(), written.refused.is_empty()) {
        (false, _) => Outcome::Fail(failures),
        (true, true) => Outcome::Pass { rows: rows.len() },
        (true, false) => Outcome::Refused(written.refused),
    }
}

fn refused_or_failed(error: String) -> Outcome {
    match error.strip_prefix(REFUSED) {
        Some(reason) => Outcome::Refused(vec![reason.to_string()]),
        None => Outcome::Fail(vec![error]),
    }
}

fn same_rows(expected: &[&RefereeRow], actual: &[RefereeRow], shift: i64) -> Vec<String> {
    let found: BTreeMap<i64, &RefereeRow> = actual.iter().map(|row| (row.id, row)).collect();
    let mut failures = Vec::new();
    for row in expected {
        match found.get(&(row.id + shift)) {
            None => failures.push(format!("id {}: no row {} on the server", row.id, row.id + shift)),
            Some(written) if written.type_name == row.type_name && written.columns == row.columns => {}
            Some(written) => failures.push(format!(
                "id {}: server stored {:?} {:?}, original {:?} {:?}",
                row.id,
                written.type_name.as_deref().unwrap_or("?"),
                short(&written.columns),
                row.type_name.as_deref().unwrap_or("?"),
                short(&row.columns)
            )),
        }
    }
    failures
}

fn short(columns: &BTreeMap<crate::rules::RefereeColumn, Option<String>>) -> Vec<String> {
    columns
        .values()
        .map(|value| match value {
            None => "NULL".to_string(),
            Some(text) if text.chars().count() > 60 => format!(
                "{}... ({} chars)",
                text.chars().take(60).collect::<String>(),
                text.chars().count()
            ),
            Some(text) => format!("{text:?}"),
        })
        .collect()
}
