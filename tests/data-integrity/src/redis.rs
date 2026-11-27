use std::time::Duration;

use bookie_core::{ConnectOptions, Connection, TlsConfig, Value};
use bookie_policy::PolicyGuard;
use serde::Deserialize;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis;

use crate::corpus::{KnownFailing, Route, SkippedRoute};
use crate::governed::{self, control};
use crate::report::{CaseResult, Outcome, Verdict};
use crate::server::{Cli, ContainerCli, Referee, labelled};

pub const TAG: &str = "7.4.12-alpine3.22";
const PREVIEW_LIMIT: usize = 4096;
const FLATTEN: &str = "local function hex(s) return (s:gsub('.', function(c) return string.format('%02x', string.byte(c)) end)) end \
local args = {} for i, a in ipairs(ARGV) do args[i] = (a:sub(2):gsub('%x%x', function(h) return string.char(tonumber(h, 16)) end)) end \
local out = {} \
local function walk(x) \
if type(x) == 'table' then \
if x.ok then table.insert(out, 's' .. hex(x.ok)) elseif x.err then table.insert(out, 'e' .. hex(x.err)) else for _, y in ipairs(x) do walk(y) end end \
elseif type(x) == 'number' then table.insert(out, 'i' .. string.format('%.0f', x)) \
elseif x == false then table.insert(out, 'N') \
else table.insert(out, 'b' .. hex(x)) end end \
walk(redis.pcall(unpack(args))) \
return table.concat(out, ':')";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisCase {
    pub id: String,
    pub setup: Vec<Vec<String>>,
    pub read: Vec<String>,
    #[serde(default)]
    pub skip: Vec<SkippedRoute>,
    #[serde(default)]
    pub known_failing: Vec<KnownFailing>,
    #[serde(default)]
    pub note: Option<String>,
}

impl RedisCase {
    pub fn key(&self) -> String {
        format!("di:{}", self.id)
    }

    fn args(&self, template: &[String]) -> Vec<Vec<u8>> {
        template.iter().map(|arg| argument(arg, &self.key())).collect()
    }

    fn known(&self, route: Route, server: &str) -> Option<&KnownFailing> {
        self.known_failing.iter().find(|known| {
            known.route == route && (known.servers.is_empty() || known.servers.iter().any(|s| s == server))
        })
    }
}

fn argument(template: &str, key: &str) -> Vec<u8> {
    match template.strip_prefix("hex:") {
        Some(hex) => hex::decode(hex).unwrap_or_else(|error| panic!("bad hex argument {hex}: {error}")),
        None => template.replace("{key}", key).into_bytes(),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisCorpus {
    pub engine: String,
    pub servers: Vec<String>,
    #[serde(rename = "case")]
    pub cases: Vec<RedisCase>,
}

pub fn load() -> RedisCorpus {
    toml::from_str(include_str!("../../data/integrity/redis.toml"))
        .unwrap_or_else(|error| panic!("parse redis.toml: {error}"))
}

pub fn validate(corpus: &RedisCorpus) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for case in &corpus.cases {
        if !ids.insert(case.id.as_str()) {
            problems.push(format!("{}: duplicate case id", case.id));
        }
        if case.read.is_empty() || case.setup.is_empty() {
            problems.push(format!("{}: a case needs setup and read commands", case.id));
        }
        for known in &case.known_failing {
            if known.issue == 0 || known.reason.trim().is_empty() {
                problems.push(format!(
                    "{}: a known failure needs an issue number and a reason",
                    case.id
                ));
            }
        }
    }
    problems
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Bulk(Vec<u8>),
    Status(String),
    Int(String),
    Error(String),
    Nil,
}

struct RedisCli;

impl Cli for RedisCli {
    fn command(&self, hex_args: &str) -> Vec<String> {
        let mut argv = vec![
            "redis-cli".to_string(),
            "--raw".into(),
            "EVAL".into(),
            FLATTEN.into(),
            "0".into(),
        ];
        argv.extend(hex_args.split(' ').filter(|arg| !arg.is_empty()).map(String::from));
        argv
    }
}

pub struct RedisServer {
    pub label: String,
    pub options: ConnectOptions,
    referee: Box<dyn Referee>,
}

impl RedisServer {
    pub async fn call(&self, args: &[Vec<u8>]) -> Result<Vec<Reply>, String> {
        let encoded: Vec<String> = args.iter().map(|arg| format!("x{}", hex::encode(arg))).collect();
        let output = self.referee.run(&encoded.join(" ")).await?;
        parse_flat(output.trim_end_matches('\n'))
    }
}

pub fn parse_flat(output: &str) -> Result<Vec<Reply>, String> {
    if output.is_empty() {
        return Ok(Vec::new());
    }
    output
        .split(':')
        .map(|token| {
            let (tag, body) = token.split_at(1.min(token.len()));
            let text = || hex::decode(body).map_err(|error| format!("referee token {token:?}: {error}"));
            Ok(match tag {
                "b" => Reply::Bulk(text()?),
                "s" => Reply::Status(String::from_utf8_lossy(&text()?).into_owned()),
                "e" => Reply::Error(String::from_utf8_lossy(&text()?).into_owned()),
                "i" => Reply::Int(body.to_string()),
                "N" => Reply::Nil,
                other => return Err(format!("unknown referee token {other:?}")),
            })
        })
        .collect()
}

pub async fn start(label: &str) -> RedisServer {
    let container = labelled(Redis::default().with_tag(TAG))
        .with_startup_timeout(Duration::from_secs(120))
        .start()
        .await
        .expect("start redis container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(6379).await.expect("port"),
        database: "0".into(),
        password: secrecy::SecretString::new(String::new().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    RedisServer {
        label: label.to_string(),
        options,
        referee: Box::new(ContainerCli {
            container,
            cli: Box::new(RedisCli),
        }),
    }
}

pub fn flatten(values: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for value in values {
        match value {
            Value::Json(json) => flatten_json(json, &mut out),
            other => out.push(other.clone()),
        }
    }
    out
}

fn flatten_json(json: &serde_json::Value, out: &mut Vec<Value>) {
    match json {
        serde_json::Value::Array(items) => items.iter().for_each(|item| flatten_json(item, out)),
        serde_json::Value::String(text) => out.push(Value::Text(text.clone())),
        serde_json::Value::Null => out.push(Value::Null),
        serde_json::Value::Number(number) => out.push(
            number
                .as_i64()
                .map_or_else(|| Value::Json(Box::new(json.clone())), Value::Int),
        ),
        other => out.push(Value::Json(Box::new(other.clone()))),
    }
}

pub fn compare(replies: &[Reply], values: &[Value]) -> Vec<String> {
    let mut failures = Vec::new();
    if replies.len() != values.len() {
        failures.push(format!(
            "server returned {} elements, BookiE {}",
            replies.len(),
            values.len()
        ));
    }
    for (index, (reply, value)) in replies.iter().zip(values).enumerate() {
        if let Err(error) = element_matches(reply, value) {
            failures.push(format!("element {index}: {error}"));
        }
    }
    failures
}

fn element_matches(reply: &Reply, value: &Value) -> Result<(), String> {
    let same = match (reply, value) {
        (Reply::Bulk(bytes), Value::Text(text)) => std::str::from_utf8(bytes).is_ok_and(|server| server == text),
        (Reply::Bulk(bytes), Value::Bytes(ours)) => std::str::from_utf8(bytes).is_err() && bytes == ours,
        (Reply::Bulk(bytes), Value::Float(ours)) => std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| text.parse::<f64>().ok())
            .is_some_and(|theirs| theirs.to_bits() == ours.to_bits() || (theirs.is_nan() && ours.is_nan())),
        (Reply::Status(server), Value::Text(text)) => server == text,
        (Reply::Int(server), Value::Int(ours)) => *server == ours.to_string(),
        (Reply::Nil, Value::Null) => true,
        _ => false,
    };
    if same {
        Ok(())
    } else {
        Err(format!(
            "server {}, BookiE {}",
            describe(reply),
            crate::rules::describe(value)
        ))
    }
}

fn describe(reply: &Reply) -> String {
    match reply {
        Reply::Bulk(bytes) if bytes.len() > 60 => format!("Bulk({} bytes)", bytes.len()),
        Reply::Bulk(bytes) => format!("Bulk({:?})", String::from_utf8_lossy(bytes)),
        other => format!("{other:?}"),
    }
}

fn command_text(args: &[Vec<u8>]) -> String {
    args.iter()
        .map(|arg| {
            let text = String::from_utf8_lossy(arg);
            if text.is_empty() || text.contains([' ', '"', '\'']) {
                format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                text.into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn run(server: &RedisServer, corpus: &RedisCorpus) -> Vec<CaseResult> {
    let mut prepared = Vec::new();
    for case in &corpus.cases {
        prepared.push((case, prepare(server, case).await));
    }
    let governed = governed::open_confirming("redis", server.options.clone()).await;
    let mut results = Vec::new();
    for (case, referee) in &prepared {
        for route in [Route::Command, Route::FullValue, Route::Browse] {
            let outcome = match (referee, case.skip.iter().find(|skip| skip.route == route)) {
                (Err(error), _) => Outcome::Broken(error.clone()),
                (Ok(_), Some(skip)) => Outcome::Skipped(skip.reason.clone()),
                (Ok(expected), None) => route_outcome(&governed.guard, case, route, expected).await,
            };
            results.push(CaseResult {
                server: server.label.clone(),
                case: case.id.clone(),
                route,
                outcome,
            });
        }
    }
    governed.close().await;
    results
}

pub struct Expected {
    read: Vec<Reply>,
    key_type: String,
    whole: Option<Vec<u8>>,
    cardinality: Option<String>,
}

async fn prepare(server: &RedisServer, case: &RedisCase) -> Result<Expected, String> {
    let key = case.key().into_bytes();
    server.call(&[b"DEL".to_vec(), key.clone()]).await?;
    for command in &case.setup {
        let replies = server.call(&case.args(command)).await?;
        if let Some(Reply::Error(error)) = replies.first() {
            return Err(format!("setup {command:?} failed: {error}"));
        }
    }
    let read = server.call(&case.args(&case.read)).await?;
    let key_type = match server.call(&[b"TYPE".to_vec(), key.clone()]).await?.first() {
        Some(Reply::Status(name)) => name.clone(),
        other => return Err(format!("TYPE returned {other:?}")),
    };
    let whole = match server.call(&[b"GET".to_vec(), key.clone()]).await?.first() {
        Some(Reply::Bulk(bytes)) => Some(bytes.clone()),
        _ => None,
    };
    let counter = match key_type.as_str() {
        "hash" => Some("HLEN"),
        "list" => Some("LLEN"),
        "set" => Some("SCARD"),
        "zset" => Some("ZCARD"),
        _ => None,
    };
    let cardinality = match counter {
        Some(command) => match server.call(&[command.as_bytes().to_vec(), key]).await?.first() {
            Some(Reply::Int(count)) => Some(count.clone()),
            other => return Err(format!("{command} returned {other:?}")),
        },
        None => None,
    };
    Ok(Expected {
        read,
        key_type,
        whole,
        cardinality,
    })
}

async fn route_outcome(guard: &PolicyGuard, case: &RedisCase, route: Route, expected: &Expected) -> Outcome {
    let failures = match route {
        Route::Command => read_command(guard, case, expected).await,
        Route::FullValue => read_full_value(guard, case, expected).await,
        _ => read_browse(guard, case, expected).await,
    };
    match failures {
        Ok(()) => Outcome::Pass { rows: 1 },
        Err(failures) => Outcome::Fail(failures),
    }
}

async fn read_command(guard: &PolicyGuard, case: &RedisCase, expected: &Expected) -> Result<(), Vec<String>> {
    let text = command_text(&case.args(&case.read));
    let batch = guard
        .query_result_sets_controlled(&text, &[], &control())
        .await
        .map_err(|error| vec![format!("{text}: {error}")])?;
    let cells: Vec<Value> = batch
        .result_sets
        .iter()
        .flat_map(|set| set.rows.iter().flat_map(|row| row.to_vec()).collect::<Vec<_>>())
        .collect();
    let failures = compare(&expected.read, &flatten(&cells));
    if failures.is_empty() { Ok(()) } else { Err(failures) }
}

async fn read_full_value(guard: &PolicyGuard, case: &RedisCase, expected: &Expected) -> Result<(), Vec<String>> {
    let Some(whole) = &expected.whole else {
        let result = guard
            .query_params_controlled(
                "SELECT \"Value\" FROM \"db0\" WHERE \"Key\" = ?",
                &[Value::Text(case.key())],
                &control(),
            )
            .await;
        return match (result, expected.key_type.as_str()) {
            (Ok(found), "none") if found.rows.is_empty() => Ok(()),
            (Err(_), kind) if kind != "none" => Ok(()),
            (Ok(found), kind) => Err(vec![format!(
                "a {kind} key returned {} full-value rows",
                found.rows.len()
            )]),
            (Err(error), kind) => Err(vec![format!("a {kind} key failed: {error}")]),
        };
    };
    let found = guard
        .query_params_controlled(
            "SELECT \"Value\" FROM \"db0\" WHERE \"Key\" = ?",
            &[Value::Text(case.key())],
            &control(),
        )
        .await
        .map_err(|error| vec![format!("full value read failed: {error}")])?;
    let cells: Vec<Value> = found.rows.iter().flat_map(|row| row.to_vec()).collect();
    let failures = compare(&[Reply::Bulk(whole.clone())], &cells);
    if failures.is_empty() { Ok(()) } else { Err(failures) }
}

async fn read_browse(guard: &PolicyGuard, case: &RedisCase, expected: &Expected) -> Result<(), Vec<String>> {
    let page = guard
        .fetch_rows_controlled(None, "db0", 0, 1000, &control())
        .await
        .map_err(|error| vec![format!("browse failed: {error}")])?;
    let position = |name: &str| page.columns.iter().position(|column| column.name == name);
    let (Some(key), Some(kind), Some(count), Some(value)) = (
        position("Key"),
        position("Type"),
        position("Cardinality"),
        position("Value"),
    ) else {
        return Err(vec![
            "browse result lacks the Key, Type, Cardinality or Value column".into(),
        ]);
    };
    let Some(row) = page.rows.iter().find(|row| row[key] == Value::Text(case.key())) else {
        return Err(vec![format!("browse does not list {}", case.key())]);
    };
    let mut failures = Vec::new();
    if row[kind] != Value::Text(expected.key_type.clone()) {
        failures.push(format!("type: server {}, BookiE {:?}", expected.key_type, row[kind]));
    }
    let count_ok = match &expected.cardinality {
        Some(server) => row[count] == Value::Int(server.parse().unwrap_or(-1)),
        None => true,
    };
    if !count_ok {
        failures.push(format!(
            "cardinality: server {:?}, BookiE {:?}",
            expected.cardinality, row[count]
        ));
    }
    match &expected.whole {
        Some(whole) if whole.len() < PREVIEW_LIMIT => {
            failures.extend(compare(&[Reply::Bulk(whole.clone())], &row[value..=value]))
        }
        Some(whole) if row[value] == bookie_value(whole) => {
            failures.push("a large value preview claims to be the whole value".into())
        }
        Some(_) => {}
        None if !matches!(row[value], Value::Undecodable(_)) => failures.push(format!(
            "a {} key shows {:?} instead of a non-value marker",
            expected.key_type, row[value]
        )),
        None => {}
    }
    if failures.is_empty() { Ok(()) } else { Err(failures) }
}

fn bookie_value(bytes: &[u8]) -> Value {
    match std::str::from_utf8(bytes) {
        Ok(text) => Value::Text(text.to_string()),
        Err(_) => Value::Bytes(bytes.to_vec()),
    }
}

pub fn judge(corpus: &RedisCorpus, results: &[CaseResult]) -> Verdict {
    let verdict = crate::report::judge_with(results, |result| {
        corpus
            .cases
            .iter()
            .find(|case| case.id == result.case)
            .and_then(|case| case.known(result.route, &result.server))
    });
    eprintln!("{}", verdict.summary());
    verdict
}
