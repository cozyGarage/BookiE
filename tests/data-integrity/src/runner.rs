use std::collections::BTreeMap;

use bookie_core::{Connection, QueryResult, Value};
use bookie_policy::PolicyGuard;
use testcontainers::Image;

use crate::corpus::{Case, Corpus, Route};
use crate::governed::{self, control};
use crate::report::{CaseResult, Outcome};
use crate::rules::{self, RefereeColumn, Rule};
use crate::server::{Dialect, Server};

pub type Observed = Result<BTreeMap<i64, Value>, String>;

#[derive(Debug, Clone)]
pub struct RefereeRow {
    pub id: i64,
    pub type_name: Option<String>,
    pub columns: BTreeMap<RefereeColumn, Option<String>>,
}

pub async fn run_reads<I: Image>(server: &Server<I>, corpus: &Corpus) -> Vec<CaseResult> {
    let governed = governed::open(server.dialect.driver_id, server.options.clone(), true).await;
    let mut results = Vec::new();
    for case in corpus.cases.iter().filter(|case| case.applies_to(&server.label)) {
        results.extend(run_case(server, &governed.guard, case).await);
    }
    results
}

async fn run_case<I: Image>(server: &Server<I>, guard: &PolicyGuard, case: &Case) -> Vec<CaseResult> {
    if let Err(error) = prepare_case(server, case).await {
        return fail_all(&server.label, case, &error);
    }
    let referee = match referee_rows(server, case).await {
        Ok(rows) => rows,
        Err(error) => return fail_all(&server.label, case, &format!("referee failed: {error}")),
    };
    let table = case.table();
    let dialect = server.dialect;
    let queried = read_query(guard, &table).await;
    let mut results = Vec::new();
    for route in Route::READS {
        let outcome = match case.skip_reason(route) {
            Some(reason) => Outcome::Skipped(reason.to_string()),
            None => {
                let observed = match route {
                    Route::Query => queried.clone(),
                    Route::Browse => read_browse(guard, dialect, &table).await,
                    Route::ParamWhere => read_param_where(guard, dialect, &table, &referee).await,
                    Route::ParamSelect => read_param_select(guard, dialect, case, &queried).await,
                };
                let observed = server_cast(server, case, route, observed).await;
                compare_rows(case, route, &referee, observed)
            }
        };
        results.push(CaseResult {
            server: server.label.clone(),
            case: case.id.clone(),
            route,
            outcome,
        });
    }
    results
}

pub async fn prepare_case<I: Image>(server: &Server<I>, case: &Case) -> Result<(), String> {
    let table = case.table();
    let dialect = server.dialect;
    server
        .run_sql(&Dialect::fill(dialect.drop_table, &table, &case.column_type))
        .await
        .map_err(|error| format!("drop failed: {error}"))?;
    for statement in &case.setup {
        server
            .run_sql(&Dialect::fill(statement, &table, &case.column_type))
            .await
            .map_err(|error| format!("setup failed: {error}"))?;
    }
    let create = format!(
        "CREATE TABLE {table} (id {} PRIMARY KEY, v {})",
        dialect.id_type,
        case.column_type(&table)
    );
    server
        .run_sql(&create)
        .await
        .map_err(|error| format!("create failed: {error}"))?;
    let session = case
        .session
        .as_deref()
        .map_or(String::new(), |session| format!("{session}; "));
    for (index, literal) in case.literals.iter().enumerate() {
        let insert = format!("{session}INSERT INTO {table} (id, v) VALUES ({}, {literal})", index + 1);
        server
            .run_sql(&insert)
            .await
            .map_err(|error| format!("literal {literal} rejected: {error}"))?;
    }
    Ok(())
}

pub fn referee_columns(case: &Case) -> Vec<RefereeColumn> {
    let mut columns = vec![case.rule.referee_column()];
    for route in Route::READS {
        let column = case.rule_for(route).referee_column();
        if !columns.contains(&column) {
            columns.push(column);
        }
    }
    columns
}

fn referee_expression(dialect: &Dialect, case: &Case, column: RefereeColumn) -> String {
    if column == case.rule.referee_column()
        && let Some(custom) = &case.referee
    {
        return Dialect::on(custom, "v");
    }
    let template = match column {
        RefereeColumn::Text => dialect.text_expr,
        RefereeColumn::Hex => dialect.hex_expr,
        RefereeColumn::Epoch => dialect.epoch_expr,
    };
    Dialect::on(template, "v")
}

pub async fn referee_rows<I: Image>(server: &Server<I>, case: &Case) -> Result<Vec<RefereeRow>, String> {
    let dialect = server.dialect;
    let table = case.table();
    let columns = referee_columns(case);
    let mut fields = vec![
        Dialect::on(dialect.text_expr, "id"),
        (dialect.wrap_hex)(&Dialect::fill(dialect.type_expr, &table, "")),
    ];
    fields.extend(
        columns
            .iter()
            .map(|column| (dialect.wrap_hex)(&referee_expression(dialect, case, *column))),
    );
    let sql = format!("SELECT {} FROM {table} ORDER BY id", (dialect.join)(&fields));
    let output = server.run_sql(&sql).await?;
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| parse_referee_line(line.trim(), &columns))
        .collect()
}

fn parse_referee_line(line: &str, columns: &[RefereeColumn]) -> Result<RefereeRow, String> {
    let mut parts = line.split(':');
    let id = parts
        .next()
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| format!("referee line without an id: {line:?}"))?;
    let type_name = decode_field(parts.next().unwrap_or("N"))?;
    let mut decoded = BTreeMap::new();
    for column in columns {
        let field = parts.next().ok_or_else(|| format!("referee line is short: {line:?}"))?;
        decoded.insert(*column, decode_field(field)?);
    }
    Ok(RefereeRow {
        id,
        type_name,
        columns: decoded,
    })
}

fn decode_field(field: &str) -> Result<Option<String>, String> {
    if field == "N" {
        return Ok(None);
    }
    let bytes = hex::decode(field).map_err(|error| format!("referee field {field:?} is not hex: {error}"))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| format!("referee text is not UTF-8: {error}"))
}

fn rows_by_id(result: &QueryResult, id_index: usize, value_index: usize) -> Observed {
    let mut rows = BTreeMap::new();
    for row in result.rows.iter() {
        let Some(Value::Int(id)) = row.get(id_index) else {
            return Err(format!("row id is not an integer: {:?}", row.get(id_index)));
        };
        let value = row.get(value_index).cloned().ok_or("row has no value column")?;
        rows.insert(*id, value);
    }
    if result.truncated {
        return Err("result was truncated".into());
    }
    Ok(rows)
}

async fn read_query(guard: &PolicyGuard, table: &str) -> Observed {
    let sql = format!("SELECT id, v FROM {table} ORDER BY id");
    let batch = guard
        .query_result_sets_controlled(&sql, &[], &control())
        .await
        .map_err(|error| format!("query failed: {error}"))?;
    let result = batch.result_sets.first().ok_or("query returned no result set")?;
    rows_by_id(result, 0, 1)
}

async fn read_browse(guard: &PolicyGuard, dialect: &Dialect, table: &str) -> Observed {
    let result = guard
        .fetch_rows_controlled(dialect.schema, table, 0, 1000, &control())
        .await
        .map_err(|error| format!("browse failed: {error}"))?;
    let position = |name: &str| result.columns.iter().position(|column| column.name == name);
    let (Some(id), Some(value)) = (position("id"), position("v")) else {
        return Err("browse result lacks the id or v column".into());
    };
    rows_by_id(&result, id, value)
}

async fn read_param_where(guard: &PolicyGuard, dialect: &Dialect, table: &str, referee: &[RefereeRow]) -> Observed {
    let sql = Dialect::fill(dialect.param_where, table, "");
    let mut rows = BTreeMap::new();
    for row in referee {
        let result = guard
            .query_params_controlled(&sql, &[Value::Int(row.id)], &control())
            .await
            .map_err(|error| format!("id {}: parameterised read failed: {error}", row.id))?;
        rows.insert(row.id, single_value(&result, row.id)?);
    }
    Ok(rows)
}

async fn read_param_select(guard: &PolicyGuard, dialect: &Dialect, case: &Case, queried: &Observed) -> Observed {
    let queried = queried.as_ref().map_err(|error| format!("no bound values: {error}"))?;
    let template = case.param.as_deref().unwrap_or(dialect.param_select);
    let table = case.table();
    let sql = Dialect::fill(template, &table, &case.column_type(&table));
    let mut rows = BTreeMap::new();
    for (id, value) in queried {
        let result = guard
            .query_params_controlled(&sql, std::slice::from_ref(value), &control())
            .await
            .map_err(|error| format!("id {id}: binding {} failed: {error}", rules::describe(value)))?;
        rows.insert(*id, single_value(&result, *id)?);
    }
    Ok(rows)
}

async fn server_cast<I: Image>(server: &Server<I>, case: &Case, route: Route, observed: Observed) -> Observed {
    if case.rule_for(route) != Rule::ServerCast {
        return observed;
    }
    let dialect = server.dialect;
    let column_type = case.column_type(&case.table());
    let mut rendered = BTreeMap::new();
    for (id, value) in observed? {
        let Value::Text(text) = &value else {
            rendered.insert(id, value);
            continue;
        };
        let cast = format!("CAST({} AS {column_type})", (dialect.quote_text)(text));
        let sql = format!("SELECT {}", (dialect.wrap_hex)(&Dialect::on(dialect.text_expr, &cast)));
        let output = server
            .run_sql(&sql)
            .await
            .map_err(|error| format!("id {id}: the server refuses BookiE's text {text:?}: {error}"))?;
        let text = decode_field(output.trim())?.unwrap_or_default();
        rendered.insert(id, Value::Text(text));
    }
    Ok(rendered)
}

fn single_value(result: &QueryResult, id: i64) -> Result<Value, String> {
    let rows: Vec<_> = result.rows.iter().collect();
    match rows.as_slice() {
        [row] if row.len() == 1 => Ok(row[0].clone()),
        other => Err(format!(
            "id {id}: expected one row with one column, got {} rows",
            other.len()
        )),
    }
}

fn compare_rows(case: &Case, route: Route, referee: &[RefereeRow], observed: Observed) -> Outcome {
    let observed = match observed {
        Ok(rows) => rows,
        Err(error) => return Outcome::Fail(vec![error]),
    };
    let rule = case.rule_for(route);
    let carrier = case.carrier_for(route);
    let mut failures = Vec::new();
    for row in referee {
        let server = row.columns.get(&rule.referee_column()).cloned().flatten();
        match observed.get(&row.id) {
            None => failures.push(format!("id {}: row missing", row.id)),
            Some(value) => {
                if let Err(error) = rules::check(rule, carrier, value, server.as_deref()) {
                    failures.push(format!("id {} ({}): {error}", row.id, literal_of(case, row.id)));
                }
            }
        }
    }
    if observed.len() != referee.len() {
        failures.push(format!(
            "BookiE returned {} rows, server has {}",
            observed.len(),
            referee.len()
        ));
    }
    if failures.is_empty() {
        Outcome::Pass { rows: referee.len() }
    } else {
        Outcome::Fail(failures)
    }
}

fn literal_of(case: &Case, id: i64) -> String {
    let literal = usize::try_from(id - 1)
        .ok()
        .and_then(|index| case.literals.get(index))
        .map_or("?", String::as_str);
    let short: String = literal.chars().take(60).collect();
    short
}

fn fail_all(server: &str, case: &Case, error: &str) -> Vec<CaseResult> {
    Route::READS
        .iter()
        .map(|route| CaseResult {
            server: server.to_string(),
            case: case.id.clone(),
            route: *route,
            outcome: Outcome::Broken(error.to_string()),
        })
        .collect()
}
