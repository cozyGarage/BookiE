use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bookie_app_services::browse_query::{BrowseTarget, PageQuery};
use bookie_app_services::cell_save_plan::{CellChange, SaveTarget, plan_cell_saves};
use bookie_app_services::export_pages::{PagePlan, PageRunner, page_fetcher};
use bookie_app_services::result_copy::{CopyContext, CopyFormat, render};
use bookie_app_services::stream_result_spool::StreamResultSpoolManager;
use bookie_app_services::value_display::{DisplayPrefs, copy_text, format_temporal_for_edit, paint_value};
use bookie_app_services::value_input::{ParseHooks, parse_grid_cell_input_with_style, parse_pasted_cell_with_style};
use bookie_core::{ColumnInfo, Connection, DatabaseDriver, FilterSet, QueryResult, TableInfo, Value};
use tokio_util::sync::CancellationToken;

use super::accounting::{Case, Key, TypeCases};

const TABLE: &str = "bookie_corpus";
const EDITED: i64 = 100_000;
const PASTED: i64 = 200_000;
const GENERATED: i64 = 300_000;

pub struct Engine<'a> {
    pub id: &'a str,
    pub driver: &'a dyn DatabaseDriver,
    pub connection: Arc<dyn Connection>,
}

type Failures = BTreeSet<Key>;

#[derive(Default)]
pub struct Report {
    pub failures: Failures,
    pub details: Vec<String>,
    entered: BTreeSet<(String, &'static str)>,
}

struct Run<'a> {
    engine: &'a Engine<'a>,
    data_type: &'a str,
    failures: Failures,
    entered: BTreeSet<&'static str>,
    details: Vec<String>,
}

impl Run<'_> {
    fn fail(&mut self, step: &str, signature: &str, detail: impl std::fmt::Debug) {
        self.details.push(format!(
            "{} {} {step} {signature}: {detail:?}",
            self.engine.id, self.data_type
        ));
        self.failures.insert(Key {
            engine: self.engine.id.into(),
            data_type: self.data_type.into(),
            step: step.into(),
            signature: signature.into(),
        });
    }

    fn sql(&self) -> &dyn Connection {
        self.engine.connection.as_ref()
    }
}

pub fn load_corpus(engine: &str) -> Vec<TypeCases> {
    let path = corpus_dir().join(format!("{engine}.json"));
    let mut types: Vec<TypeCases> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let contract: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_dir().join("../value-contract.json")).unwrap()).unwrap();
    for entry in &mut types {
        let Some(list) = entry.contract.as_deref() else {
            continue;
        };
        for text in contract[list].as_array().unwrap() {
            let text = text.as_str().unwrap();
            let value = match list {
                "integers" => Value::Int(text.parse().unwrap()),
                "texts" => Value::Text(text.into()),
                other => panic!("unknown value contract list {other}"),
            };
            let literal = bookie_core::sql_literal::render_sql_literal(engine, &value).unwrap();
            entry.cases.push(Case {
                literal,
                cell: text.into(),
            });
        }
    }
    types
}

pub fn load_known_issues() -> BTreeMap<Key, u64> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Known {
        engine: String,
        data_type: String,
        step: String,
        signature: String,
        issue: u64,
    }
    let text = std::fs::read_to_string(corpus_dir().join("known-issues.json")).unwrap();
    let known: Vec<Known> = serde_json::from_str(&text).unwrap();
    known
        .into_iter()
        .map(|entry| {
            let key = Key {
                engine: entry.engine,
                data_type: entry.data_type,
                step: entry.step,
                signature: entry.signature,
            };
            (key, entry.issue)
        })
        .collect()
}

fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/type-corpus")
}

pub async fn run_engine(engine: &Engine<'_>, corpus: &[TypeCases]) -> Report {
    let mut report = Report::default();
    for entry in corpus {
        let mut run = Run {
            engine,
            data_type: &entry.data_type,
            failures: Failures::new(),
            entered: BTreeSet::new(),
            details: Vec::new(),
        };
        run_type(&mut run, &entry.cases).await;
        report.failures.append(&mut run.failures);
        report.details.append(&mut run.details);
        for step in run.entered {
            report.entered.insert((entry.data_type.clone(), step));
        }
    }
    report
}

async fn run_type(run: &mut Run<'_>, cases: &[Case]) {
    let _ = run.sql().execute(&format!("DROP TABLE IF EXISTS {TABLE}")).await;
    let create = format!("CREATE TABLE {TABLE} (id INTEGER PRIMARY KEY, v {})", run.data_type);
    if let Err(error) = run.sql().execute(&create).await {
        return run.fail("create", "error", error);
    }
    for (index, case) in cases.iter().enumerate() {
        let insert = format!("INSERT INTO {TABLE} (id, v) VALUES ({}, {})", index + 1, case.literal);
        if let Err(error) = run.sql().execute(&insert).await {
            run.fail("insert", "error", (&case.literal, error));
        }
    }
    let columns = match run.sql().fetch_columns(None, TABLE).await {
        Ok(columns) if columns.len() == 2 => columns,
        other => return run.fail("columns", "error", other),
    };
    let Some(rows) = browse(run, &columns).await else {
        return;
    };
    let values: Vec<(i64, Value)> = (1..=cases.len() as i64)
        .filter_map(|id| rows.get(&id).map(|value| (id, value.clone())))
        .collect();
    check_cells(run, cases, &rows);
    check_stream(run, &columns, &rows).await;
    check_export(run, &columns, &rows).await;
    let mut expected = BTreeMap::new();
    enter_and_save(run, Entry::Edit, &columns, &values, &mut expected).await;
    enter_and_save(run, Entry::Paste, &columns, &values, &mut expected).await;
    generate_inserts(run, &columns, &values, &mut expected).await;
    let Some(reread) = browse(run, &columns).await else {
        return;
    };
    for (id, (step, value)) in expected {
        match reread.get(&id) {
            Some(actual) if same(actual, &value) => {}
            other => run.fail(step, "round-trip-mismatch", (value, other)),
        }
    }
}

fn check_cells(run: &mut Run<'_>, cases: &[Case], rows: &BTreeMap<i64, Value>) {
    for (index, case) in cases.iter().enumerate() {
        let Some(value) = rows.get(&(index as i64 + 1)) else {
            run.fail("browse", "missing-row", &case.literal);
            continue;
        };
        let painted = paint_value(value, DisplayPrefs::default());
        if painted.as_str() != case.cell {
            run.fail("browse", "cell-mismatch", (&case.literal, &case.cell, painted.as_str()));
        }
    }
}

async fn browse(run: &mut Run<'_>, columns: &[ColumnInfo]) -> Option<BTreeMap<i64, Value>> {
    let filter = FilterSet::default();
    let target = BrowseTarget {
        driver_id: run.engine.id,
        schema: None,
        table: TABLE,
        columns,
        filter: &filter,
        hidden_columns: None,
        long_text_preview: false,
        supports_reverse_keyset: false,
    };
    let result = match target.page(0, 1_000, Some((0, true)), None) {
        Ok(PageQuery::Native) => run.sql().fetch_rows(None, TABLE, 0, 1_000).await,
        Ok(PageQuery::Sql(query)) => run.sql().query_params(&query.sql, &query.params).await,
        Err(error) => {
            run.fail("browse", "plan-error", error);
            return None;
        }
    };
    match result {
        Ok(result) => Some(keyed(result.rows.into_rows())),
        Err(error) => {
            run.fail("browse", "error", error);
            None
        }
    }
}

fn keyed(rows: Vec<Vec<Value>>) -> BTreeMap<i64, Value> {
    rows.into_iter()
        .filter_map(|row| match row.as_slice() {
            [Value::Int(id), value] => Some((*id, value.clone())),
            _ => None,
        })
        .collect()
}

fn as_rows(rows: &BTreeMap<i64, Value>) -> Vec<Vec<Value>> {
    rows.iter()
        .map(|(id, value)| vec![Value::Int(*id), value.clone()])
        .collect()
}

async fn check_stream(run: &mut Run<'_>, columns: &[ColumnInfo], rows: &BTreeMap<i64, Value>) {
    let spool = StreamResultSpoolManager::with_limit(64 * 1024 * 1024).create().unwrap();
    let page = Arc::new(QueryResult {
        columns: columns.to_vec(),
        rows: as_rows(rows).into(),
        truncated: false,
    });
    let loaded = match spool.append_page(0, page).await {
        Ok(()) => spool.load_page(0, columns.to_vec()).await,
        Err(error) => return run.fail("stream", "error", error),
    };
    match loaded {
        Ok(loaded) if keyed(loaded.rows.iter().map(<[Value]>::to_vec).collect()) == *rows => {}
        other => run.fail("stream", "mismatch", other.map(|result| result.rows.len())),
    }
}

async fn check_export(run: &mut Run<'_>, columns: &[ColumnInfo], rows: &BTreeMap<i64, Value>) {
    let plan = PagePlan {
        driver_id: run.engine.id.into(),
        schema: None,
        table: TABLE.into(),
        columns: columns.to_vec(),
        filter: FilterSet::default(),
        sort: None,
        row_id_order: false,
        timeout_secs: 30,
    };
    let runner: PageRunner =
        Arc::new(|page| tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(page)));
    let connection = run.engine.connection.clone();
    let exported = tokio::task::spawn_blocking(move || {
        let mut next = page_fetcher(connection, plan, runner, CancellationToken::new());
        let mut all = Vec::new();
        while let Some(page) = next()? {
            all.extend(page);
        }
        Ok::<_, bookie_core::export::ExportError>(all)
    })
    .await
    .unwrap();
    match exported {
        Ok(exported) if keyed(exported.clone()) == *rows => check_export_text(run, columns, &exported),
        other => run.fail("export", "mismatch", other),
    }
}

fn check_export_text(run: &mut Run<'_>, columns: &[ColumnInfo], rows: &[Vec<Value>]) {
    let context = CopyContext {
        columns,
        driver_id: run.engine.id,
        target: None,
        column: 1,
    };
    for format in [CopyFormat::Csv, CopyFormat::Json] {
        if let Err(error) = render(format, &context, rows) {
            run.fail("export", "render-error", (format, error));
        }
    }
}

fn hooks(engine: &Engine<'_>) -> ParseHooks {
    ParseHooks::new(
        engine.driver.capabilities().enum_input_style,
        engine.driver.value_input_capabilities(),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Entry {
    Edit,
    Paste,
}

impl Entry {
    fn step(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Paste => "copy",
        }
    }

    fn first_row(self) -> i64 {
        match self {
            Self::Edit => EDITED,
            Self::Paste => PASTED,
        }
    }

    fn parse(self, value: &Value, column: &ColumnInfo, hooks: &ParseHooks) -> Option<Result<Value, String>> {
        let copied = copy_text(value).into_string();
        let parsed = match self {
            Self::Edit => {
                let text = format_temporal_for_edit(value).unwrap_or(copied);
                if text.is_empty() {
                    return None;
                }
                parse_grid_cell_input_with_style(&text, Some(column), hooks, Some(&Value::Null))
            }
            Self::Paste => parse_pasted_cell_with_style(Some(&copied), Some(column), hooks, Some(&Value::Null)),
        };
        Some(parsed.map_err(|error| format!("{error:?}")))
    }
}

fn inline_editable(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bytes(_) | Value::Undecodable(_) => false,
        Value::Text(text) => !text.contains('\0'),
        _ => true,
    }
}

async fn enter_and_save(
    run: &mut Run<'_>,
    entry: Entry,
    columns: &[ColumnInfo],
    values: &[(i64, Value)],
    expected: &mut BTreeMap<i64, (&'static str, Value)>,
) {
    let hooks = hooks(run.engine);
    let target = SaveTarget {
        driver_id: run.engine.id,
        schema: None,
        table: TABLE,
        columns,
    };
    let grid_columns: Vec<String> = columns.iter().map(|column| column.name.clone()).collect();
    for (id, value) in values {
        if !inline_editable(value) {
            continue;
        }
        let Some(parsed) = entry.parse(value, &columns[1], &hooks) else {
            continue;
        };
        run.entered.insert(entry.step());
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                run.fail(entry.step(), "parse-error", (value, error));
                continue;
            }
        };
        let row = entry.first_row() + id;
        if let Err(error) = run
            .sql()
            .execute(&format!("INSERT INTO {TABLE} (id) VALUES ({row})"))
            .await
        {
            run.fail(entry.step(), "error", error);
            continue;
        }
        let change = CellChange {
            original_row: vec![Value::Int(row), Value::Null],
            column: 1,
            new_value: parsed,
        };
        let saved = match plan_cell_saves(target, &grid_columns, &[change]) {
            Ok(plan) => run
                .sql()
                .execute_in_transaction(&plan.statements)
                .await
                .map_err(|e| e.to_string()),
            Err(error) => Err(error.to_string()),
        };
        match saved {
            Ok(_) => _ = expected.insert(row, (entry.step(), value.clone())),
            Err(error) => run.fail(entry.step(), "save-error", error),
        }
    }
}

async fn generate_inserts(
    run: &mut Run<'_>,
    columns: &[ColumnInfo],
    values: &[(i64, Value)],
    expected: &mut BTreeMap<i64, (&'static str, Value)>,
) {
    let context = CopyContext {
        columns,
        driver_id: run.engine.id,
        target: Some(TableInfo {
            schema: None,
            name: TABLE.into(),
        }),
        column: 1,
    };
    for (id, value) in values {
        let row = GENERATED + id;
        let statement = match render(CopyFormat::Insert, &context, &[vec![Value::Int(row), value.clone()]]) {
            Ok(copied) => copied.text,
            Err(error) => {
                run.fail("sql", "render-error", (value, error));
                continue;
            }
        };
        match run.sql().execute(&statement).await {
            Ok(_) => _ = expected.insert(row, ("sql", value.clone())),
            Err(error) => run.fail("sql", "execute-error", (statement, error)),
        }
    }
}

fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Float(left), Value::Float(right)) => left.to_bits() == right.to_bits(),
        _ => left == right,
    }
}

pub fn matrix(engine: &str, corpus: &[TypeCases], report: &Report, known: &BTreeMap<Key, u64>) -> String {
    let steps = [
        "create", "insert", "columns", "browse", "stream", "export", "copy", "edit", "sql",
    ];
    let mut out = format!(
        "| {engine} | {} |\n|---|{}\n",
        steps.join(" | "),
        "---|".repeat(steps.len())
    );
    for entry in corpus {
        let cells: Vec<String> = steps
            .iter()
            .map(|step| {
                let hits: Vec<&Key> = report
                    .failures
                    .iter()
                    .filter(|key| key.data_type == entry.data_type && key.step == *step)
                    .collect();
                let entered = report.entered.contains(&(entry.data_type.clone(), *step));
                match hits.as_slice() {
                    [] if matches!(*step, "copy" | "edit") && !entered => "n/a".to_owned(),
                    [] => "ok".to_owned(),
                    hits if hits.iter().all(|key| known.contains_key(*key)) => hits
                        .iter()
                        .map(|key| format!("#{}", known[*key]))
                        .collect::<Vec<_>>()
                        .join(" "),
                    _ => "FAIL".to_owned(),
                }
            })
            .collect();
        out.push_str(&format!("| {} | {} |\n", entry.data_type, cells.join(" | ")));
    }
    out
}
