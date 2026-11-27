use std::time::Duration;

use bookie_core::{ConnectOptions, Connection, TlsConfig, Value};
use bookie_policy::PolicyGuard;
use serde::Deserialize;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis;

use crate::corpus::{KnownFailing, Route, SkippedRoute};
use crate::governed::{self, control};
use crate::redis_reply::{Node, Scores, Shape, compare, from_result, node_of, parse_cli};
use crate::report::{CaseResult, Outcome, Verdict};
use crate::server::{Cli, ContainerCli, Referee, labelled};

const COLLECTION_REFUSAL: &str = "collection values cannot be fetched";

pub const TAG: &str = "7.4.12-alpine3.22";
const PREVIEW_LIMIT: usize = 4096;
const SETUP: &str = "local args = {} for i, a in ipairs(ARGV) do args[i] = (a:sub(2):gsub('%x%x', function(h) return string.char(tonumber(h, 16)) end)) end \
local reply = redis.pcall(unpack(args)) \
if type(reply) == 'table' and reply.err then return 'ERR ' .. reply.err end \
return 'OK'";
const SEPARATOR: char = '\u{1}';

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisCase {
    pub id: String,
    pub setup: Vec<Vec<String>>,
    pub read: Vec<String>,
    #[serde(default)]
    pub scores: Scores,
    #[serde(default)]
    pub unordered: bool,
    #[serde(default)]
    pub skip: Vec<SkippedRoute>,
    #[serde(default)]
    pub known_failing: Vec<KnownFailing>,
    #[serde(default)]
    pub note: Option<String>,
}

impl RedisCase {
    fn shape(&self) -> Shape {
        Shape {
            scores: self.scores,
            unordered: self.unordered,
        }
    }

    pub fn key(&self) -> String {
        format!("di:{}", self.id)
    }

    fn args(&self, template: &[String]) -> Vec<Vec<u8>> {
        template.iter().flat_map(|arg| arguments(arg, &self.key())).collect()
    }

    fn known(&self, route: Route, server: &str) -> Option<&KnownFailing> {
        self.known_failing
            .iter()
            .find(|known| known.covers(&self.id, route, server))
    }
}

fn arguments(template: &str, key: &str) -> Vec<Vec<u8>> {
    if let Some(hex) = template.strip_prefix("hex:") {
        return vec![hex::decode(hex).unwrap_or_else(|error| panic!("bad hex argument {hex}: {error}"))];
    }
    if let Some(spec) = template.strip_prefix("seq:") {
        let (count, prefix) = spec.split_once(':').unwrap_or((spec, ""));
        let count: usize = count.parse().unwrap_or_else(|_| panic!("bad seq argument {template}"));
        return (0..count)
            .map(|index| format!("{prefix}{index}").into_bytes())
            .collect();
    }
    if let Some(spec) = template.strip_prefix("pairs:") {
        let (count, prefix) = spec.split_once(':').unwrap_or((spec, ""));
        let count: usize = count
            .parse()
            .unwrap_or_else(|_| panic!("bad pairs argument {template}"));
        return (0..count)
            .flat_map(|index| {
                [
                    format!("{prefix}{index}").into_bytes(),
                    format!("v{index}").into_bytes(),
                ]
            })
            .collect();
    }
    vec![template.replace("{key}", key).into_bytes()]
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

struct RedisCli;

impl Cli for RedisCli {
    fn command(&self, request: &str) -> Vec<String> {
        let mut parts = request.split(SEPARATOR);
        let mut argv = match parts.next() {
            Some("SETUP") => vec![
                "redis-cli".to_string(),
                "--raw".into(),
                "EVAL".into(),
                SETUP.into(),
                "0".into(),
            ],
            _ => vec!["redis-cli".to_string(), "--quoted-json".into()],
        };
        argv.extend(parts.map(String::from));
        argv
    }
}

pub struct RedisServer {
    pub label: String,
    pub options: ConnectOptions,
    referee: Box<dyn Referee>,
}

impl RedisServer {
    async fn setup(&self, args: &[Vec<u8>]) -> Result<(), String> {
        let encoded: Vec<String> = args.iter().map(|arg| format!("x{}", hex::encode(arg))).collect();
        let request = std::iter::once("SETUP".to_string()).chain(encoded).collect::<Vec<_>>();
        let output = self.referee.run(&request.join(&SEPARATOR.to_string())).await?;
        match output.trim() {
            "OK" => Ok(()),
            other => Err(other.to_string()),
        }
    }

    async fn read(&self, args: &[Vec<u8>]) -> Result<Node, String> {
        let mut request = vec!["READ".to_string()];
        for arg in args {
            let text = std::str::from_utf8(arg).map_err(|_| "a read argument must be UTF-8 text".to_string())?;
            request.push(text.to_string());
        }
        let output = self.referee.run(&request.join(&SEPARATOR.to_string())).await?;
        parse_cli(&output)
    }
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
    read: Node,
    key_type: String,
    whole: Option<Vec<u8>>,
    cardinality: Option<String>,
}

async fn prepare(server: &RedisServer, case: &RedisCase) -> Result<Expected, String> {
    let key = case.key().into_bytes();
    server.setup(&[b"DEL".to_vec(), key.clone()]).await?;
    for command in &case.setup {
        server
            .setup(&case.args(command))
            .await
            .map_err(|error| format!("setup {command:?} failed: {error}"))?;
    }
    let read = server.read(&case.args(&case.read)).await?;
    let key_type = match server.read(&[b"TYPE".to_vec(), key.clone()]).await? {
        Node::Bytes(name) => String::from_utf8_lossy(&name).into_owned(),
        other => return Err(format!("TYPE returned {other:?}")),
    };
    let whole = match server.read(&[b"GET".to_vec(), key.clone()]).await {
        Ok(Node::Bytes(bytes)) => Some(bytes),
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
        Some(command) => match server.read(&[command.as_bytes().to_vec(), key]).await? {
            Node::Int(count) => Some(count),
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
    let [result] = batch.result_sets.as_slice() else {
        return Err(vec![format!(
            "{text}: expected one result set, got {}",
            batch.result_sets.len()
        )]);
    };
    let failures = compare(&expected.read, &from_result(result), case.shape());
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
            (Err(error), kind) if kind != "none" && error.to_string().contains(COLLECTION_REFUSAL) => Ok(()),
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
    let [row] = found.rows.iter().collect::<Vec<_>>()[..] else {
        return Err(vec![format!("full value read returned {} rows", found.rows.len())]);
    };
    let failures = compare(&Node::Bytes(whole.clone()), &node_of(&row[0]), Shape::default());
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
        Some(whole) if whole.len() < PREVIEW_LIMIT => failures.extend(compare(
            &Node::Bytes(whole.clone()),
            &node_of(&row[value]),
            Shape::default(),
        )),
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
