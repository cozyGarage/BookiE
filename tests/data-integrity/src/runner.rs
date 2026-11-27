use std::collections::BTreeMap;

use bookie_core::{Connection, QueryResult, Value};
use bookie_policy::PolicyGuard;

use crate::corpus::{Case, Corpus, Route};
use crate::governed::{self, control};
use crate::report::{CaseResult, Outcome};
use crate::rules::{self, RefereeColumn, Rule};
use crate::server::{Dialect, Server, SettingCheck};

pub type Observed = Result<BTreeMap<i64, Value>, String>;

#[derive(Debug, Clone)]
pub struct RefereeRow {
    pub id: i64,
    pub type_name: Option<String>,
    pub columns: BTreeMap<RefereeColumn, Option<String>>,
}

type Prepared<'a> = (&'a Case, Result<Vec<RefereeRow>, String>);
type RouteReads = Vec<(Route, Option<Observed>)>;

pub async fn run_reads(server: &Server, corpus: &Corpus) -> Vec<CaseResult> {
    let mut prepared: Vec<Prepared<'_>> = Vec::new();
    for case in corpus.cases.iter().filter(|case| case.applies_to(&server.label)) {
        let referee = match prepare_case(server, case).await {
            Ok(()) => referee_rows(server, case)
                .await
                .map_err(|error| format!("referee failed: {error}")),
            Err(error) => Err(error),
        };
        prepared.push((case, referee));
    }
    let reads = read_all(server, &prepared).await;
    let mut results = Vec::new();
    for ((case, referee), routes) in prepared.iter().zip(reads) {
        match referee {
            Err(error) => results.extend(fail_all(&server.label, case, error)),
            Ok(rows) => results.extend(judge_case(server, case, rows, routes).await),
        }
    }
    results
}

async fn read_all(server: &Server, prepared: &[Prepared<'_>]) -> Vec<RouteReads> {
    let mut reads: Vec<RouteReads> = prepared.iter().map(|_| Vec::new()).collect();
    let governed = governed::open(server.dialect.driver_id, server.options.clone(), true).await;
    for (index, (case, referee)) in prepared.iter().enumerate() {
        if let (Ok(rows), true) = (referee, case.bookie_session.is_empty()) {
            reads[index] = read_case(&governed.guard, server.dialect, case, rows).await;
        }
    }
    governed.close().await;
    for (index, (case, referee)) in prepared.iter().enumerate() {
        if let (Ok(rows), false) = (referee, case.bookie_session.is_empty()) {
            reads[index] = read_in_session(server, case, rows).await;
        }
    }
    reads
}

async fn read_in_session(server: &Server, case: &Case, referee: &[RefereeRow]) -> RouteReads {
    let failed = |error: String| -> RouteReads {
        Route::READS
            .iter()
            .map(|route| (*route, Some(Err(error.clone()))))
            .collect()
    };
    let variant = (server.dialect.session_variant)(&case.table(), &case.bookie_session);
    for statement in &variant.setup {
        if let Err(error) = server.run_sql(statement).await {
            return failed(format!("session setup failed: {error}"));
        }
    }
    let mut options = server.options.clone();
    if let Some((username, password)) = &variant.login {
        options.username.clone_from(username);
        options.password = secrecy::SecretString::new(password.clone().into());
    }
    let governed = if variant.bookie_setup.is_empty() {
        governed::open(server.dialect.driver_id, options, true).await
    } else {
        governed::open_writable(server.dialect.driver_id, options).await
    };
    let mut reads = match apply_bookie_setup(&governed.guard, &variant.bookie_setup).await {
        Err(error) => failed(error),
        Ok(()) => match check_settings(server, &governed.guard, &variant.checks).await {
            Ok(()) => read_case(&governed.guard, server.dialect, case, referee).await,
            Err(error) => failed(error),
        },
    };
    governed.close().await;
    for statement in &variant.teardown {
        if let Err(error) = server.run_sql(statement).await {
            reads = failed(format!("session teardown failed: {error}"));
        }
    }
    reads
}

async fn apply_bookie_setup(guard: &PolicyGuard, statements: &[String]) -> Result<(), String> {
    for statement in statements {
        guard
            .query_result_sets_controlled(statement, &[], &control())
            .await
            .map_err(|error| format!("{statement}: {error}"))?;
    }
    Ok(())
}

async fn check_settings(server: &Server, guard: &PolicyGuard, checks: &[SettingCheck]) -> Result<(), String> {
    for check in checks {
        let actual = if check.through_bookie {
            let batch = guard
                .query_result_sets_controlled(&check.sql, &[], &control())
                .await
                .map_err(|error| format!("{}: {error}", check.sql))?;
            match batch.result_sets.first().and_then(|result| result.rows.iter().next()) {
                Some(row) => match row.first() {
                    Some(Value::Text(text)) => text.clone(),
                    other => format!("{other:?}"),
                },
                None => String::new(),
            }
        } else {
            server.run_sql(&check.sql).await?.trim().to_string()
        };
        if actual != check.expected {
            return Err(format!(
                "session setting was not applied: {} returned {actual:?}, expected {:?}",
                check.sql, check.expected
            ));
        }
    }
    Ok(())
}

async fn read_case(guard: &PolicyGuard, dialect: &Dialect, case: &Case, referee: &[RefereeRow]) -> RouteReads {
    let table = case.table();
    let queried = read_query(guard, &table).await;
    let mut routes = Vec::new();
    for route in Route::READS {
        if case.skip_reason(route).is_some() {
            routes.push((route, None));
            continue;
        }
        let observed = match route {
            Route::Query => queried.clone(),
            Route::Browse => read_browse(guard, dialect, &table).await,
            Route::ParamWhere => read_param_where(guard, dialect, &table, referee).await,
            Route::ParamSelect => read_param_select(guard, dialect, case, &queried).await,
            Route::Command | Route::FullValue => Err(format!("{} is not a SQL route", route.name())),
        };
        routes.push((route, Some(observed)));
    }
    routes
}

async fn judge_case(server: &Server, case: &Case, referee: &[RefereeRow], routes: RouteReads) -> Vec<CaseResult> {
    let mut results = Vec::new();
    for (route, observed) in routes {
        let outcome = match observed {
            None => Outcome::Skipped(case.skip_reason(route).unwrap_or_default().to_string()),
            Some(observed) => {
                let observed = server_cast(server, case, route, observed).await;
                expect_refusal(case, route, compare_rows(case, route, referee, observed))
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

pub fn expect_refusal(case: &Case, route: Route, outcome: Outcome) -> Outcome {
    let Some(expected) = case.expected_error(route) else {
        return outcome;
    };
    match outcome {
        Outcome::Fail(details) if details.iter().all(|detail| detail.contains(&expected.expect)) => {
            Outcome::Refused(details)
        }
        Outcome::Fail(details) => Outcome::Fail(
            std::iter::once(format!("expected an error containing {:?}", expected.expect))
                .chain(details)
                .collect(),
        ),
        Outcome::Pass { .. } => Outcome::Fail(vec![format!(
            "the route now succeeds; the expected error {:?} is gone, remove its expect_error marker",
            expected.expect
        )]),
        other => other,
    }
}

pub async fn prepare_case(server: &Server, case: &Case) -> Result<(), String> {
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
    let template = case.create.as_deref().unwrap_or(dialect.create_table);
    let create = Dialect::fill(template, &table, &case.column_type(&table));
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
    let typed = case.by_type.values().map(|expectation| expectation.rule);
    for rule in Route::READS.iter().map(|route| case.rule_for(*route)).chain(typed) {
        let column = rule.referee_column();
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

pub async fn referee_rows(server: &Server, case: &Case) -> Result<Vec<RefereeRow>, String> {
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
        .map(|line| parse_referee_line(line.trim(), &columns).map(|row| seconds_epoch(dialect, row)))
        .collect()
}

fn seconds_epoch(dialect: &Dialect, mut row: RefereeRow) -> RefereeRow {
    if dialect.epoch_in_nanos
        && let Some(Some(nanos)) = row.columns.get_mut(&RefereeColumn::Epoch)
    {
        *nanos = rules::nanos_to_seconds(nanos);
    }
    row
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

async fn server_cast(server: &Server, case: &Case, route: Route, observed: Observed) -> Observed {
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
        let render = (dialect.wrap_hex)(&Dialect::on(dialect.text_expr, "v"));
        let sql = (dialect.cast_query)(&case.bookie_session, &cast, &render);
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

pub fn compare_rows(case: &Case, route: Route, referee: &[RefereeRow], observed: Observed) -> Outcome {
    if referee.len() != case.literals.len() {
        return Outcome::Broken(format!(
            "the server referee returned {} rows for {} literals",
            referee.len(),
            case.literals.len()
        ));
    }
    let observed = match observed {
        Ok(rows) => rows,
        Err(error) => return Outcome::Fail(vec![error]),
    };
    let mut failures = Vec::new();
    for row in referee {
        let expected = case.expectation(route, row.type_name.as_deref());
        let (rule, carrier) = (expected.rule, expected.carrier);
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
