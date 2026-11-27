use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Deserialize;

use crate::rules::{Carrier, Rule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    Query,
    Browse,
    ParamWhere,
    ParamSelect,
}

impl Route {
    pub const READS: [Route; 4] = [Route::Query, Route::Browse, Route::ParamWhere, Route::ParamSelect];

    pub fn name(self) -> &'static str {
        match self {
            Route::Query => "query",
            Route::Browse => "browse",
            Route::ParamWhere => "param_where",
            Route::ParamSelect => "param_select",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownFailing {
    pub route: Route,
    pub issue: u32,
    pub expect: String,
    #[serde(default)]
    pub ids: Vec<i64>,
    #[serde(default)]
    pub servers: Vec<String>,
    #[serde(default)]
    pub cases: Vec<String>,
    pub reason: String,
}

impl KnownFailing {
    fn covers(&self, case: &str, route: Route, server: &str) -> bool {
        self.route == route
            && (self.servers.is_empty() || self.servers.iter().any(|s| s == server))
            && (self.cases.is_empty() || self.cases.iter().any(|c| c == case))
    }

    pub fn mismatch(&self, details: &[String]) -> Option<String> {
        if let Some(other) = details.iter().find(|detail| !detail.contains(&self.expect)) {
            return Some(format!("failure does not contain {:?}: {other}", self.expect));
        }
        if self.ids.is_empty() {
            return None;
        }
        let mut failing: Vec<i64> = details.iter().filter_map(|detail| row_id(detail)).collect();
        failing.sort_unstable();
        failing.dedup();
        let mut expected = self.ids.clone();
        expected.sort_unstable();
        (failing != expected).then(|| format!("rows {failing:?} fail, the marker names rows {expected:?}"))
    }
}

fn row_id(detail: &str) -> Option<i64> {
    let digits: String = detail
        .strip_prefix("id ")?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkippedRoute {
    pub route: Route,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractList {
    Integers,
    Floats,
    Decimals,
    Texts,
}

impl ContractList {
    fn key(self) -> &'static str {
        match self {
            ContractList::Integers => "integers",
            ContractList::Floats => "floats",
            ContractList::Decimals => "decimals",
            ContractList::Texts => "texts",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    #[serde(rename = "type")]
    pub column_type: String,
    #[serde(default)]
    pub literals: Vec<String>,
    #[serde(default)]
    pub literals_from: Option<ContractList>,
    pub carrier: Carrier,
    pub rule: Rule,
    #[serde(default)]
    pub servers: Vec<String>,
    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub bookie_session: Vec<String>,
    #[serde(default)]
    pub referee: Option<String>,
    #[serde(default)]
    pub param: Option<String>,
    #[serde(default)]
    pub param_carrier: Option<Carrier>,
    #[serde(default)]
    pub param_rule: Option<Rule>,
    #[serde(default)]
    pub skip: Vec<SkippedRoute>,
    #[serde(default)]
    pub known_failing: Vec<KnownFailing>,
    #[serde(default)]
    pub note: Option<String>,
}

impl Case {
    pub fn applies_to(&self, server: &str) -> bool {
        self.servers.is_empty() || self.servers.iter().any(|name| name == server)
    }

    pub fn known_issue(&self, route: Route, server: &str) -> Option<&KnownFailing> {
        self.known_failing
            .iter()
            .find(|known| known.covers(&self.id, route, server))
    }

    pub fn skip_reason(&self, route: Route) -> Option<&str> {
        self.skip
            .iter()
            .find(|skipped| skipped.route == route)
            .map(|skipped| skipped.reason.as_str())
    }

    pub fn carrier_for(&self, route: Route) -> Carrier {
        match route {
            Route::ParamSelect => self.param_carrier.unwrap_or(self.carrier),
            _ => self.carrier,
        }
    }

    pub fn rule_for(&self, route: Route) -> Rule {
        match route {
            Route::ParamSelect => self.param_rule.unwrap_or(self.rule),
            _ => self.rule,
        }
    }

    pub fn column_type(&self, table: &str) -> String {
        self.column_type.replace("{table}", table)
    }

    pub fn table(&self) -> String {
        let tail: String = self
            .id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        format!("di_{tail}")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub engine: String,
    pub servers: Vec<String>,
    #[serde(default)]
    pub known_failing: Vec<KnownFailing>,
    #[serde(rename = "case")]
    pub cases: Vec<Case>,
}

impl Corpus {
    pub fn known_issue(&self, case: &str, route: Route, server: &str) -> Option<&KnownFailing> {
        let own = self
            .cases
            .iter()
            .find(|candidate| candidate.id == case)
            .and_then(|candidate| candidate.known_issue(route, server));
        own.or_else(|| {
            self.known_failing
                .iter()
                .find(|known| !known.cases.is_empty() && known.covers(case, route, server))
        })
    }

    pub fn issue_numbers(&self) -> BTreeSet<u32> {
        self.known_failing
            .iter()
            .chain(self.cases.iter().flat_map(|case| case.known_failing.iter()))
            .map(|known| known.issue)
            .collect()
    }
}

pub const ENGINES: [&str; 2] = ["postgres", "mysql"];

const VALUE_CONTRACT: &str = include_str!("../../data/value-contract.json");

fn corpus_text(engine: &str) -> &'static str {
    match engine {
        "postgres" => include_str!("../../data/integrity/postgres.toml"),
        "mysql" => include_str!("../../data/integrity/mysql.toml"),
        other => panic!("no data-integrity corpus for {other}"),
    }
}

pub fn source_dir() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("current directory"))
        .join("../data/integrity")
}

pub fn load(engine: &str, quote_text: fn(&str) -> String) -> Corpus {
    let mut corpus: Corpus =
        toml::from_str(corpus_text(engine)).unwrap_or_else(|error| panic!("parse {engine}.toml: {error}"));
    let contract: serde_json::Value = serde_json::from_str(VALUE_CONTRACT).expect("parse value-contract.json");
    for case in &mut corpus.cases {
        if let Some(list) = case.literals_from {
            case.literals.extend(contract_literals(&contract, list, quote_text));
        }
    }
    corpus
}

fn contract_literals(contract: &serde_json::Value, list: ContractList, quote_text: fn(&str) -> String) -> Vec<String> {
    let entries = contract[list.key()]
        .as_array()
        .unwrap_or_else(|| panic!("value-contract.json has no list {}", list.key()));
    entries
        .iter()
        .map(|entry| entry.as_str().expect("value-contract entries are strings"))
        .map(|text| match list {
            ContractList::Texts => quote_text(text),
            _ => text.to_string(),
        })
        .collect()
}

pub fn validate(corpus: &Corpus) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    let mut tables = BTreeSet::new();
    for case in &corpus.cases {
        if !ids.insert(case.id.clone()) {
            problems.push(format!("{}: duplicate case id", case.id));
        }
        if !tables.insert(case.table()) {
            problems.push(format!(
                "{}: table name {} collides with another case",
                case.id,
                case.table()
            ));
        }
        if case.literals.is_empty() {
            problems.push(format!("{}: no literals", case.id));
        }
        problems.extend(unknown_servers(corpus, case));
        problems.extend(marker_problems(case));
        problems.extend(server_cast_problems(case));
    }
    problems.extend(corpus_marker_problems(corpus));
    problems
}

fn server_cast_problems(case: &Case) -> Vec<String> {
    let uses_server_cast = Route::READS
        .iter()
        .any(|route| case.rule_for(*route) == Rule::ServerCast);
    if !uses_server_cast {
        return Vec::new();
    }
    let mut problems = Vec::new();
    if case.column_type.contains('(') {
        problems.push(format!(
            "{}: server_cast cannot be used on {}: a cast to a type with a modifier rounds or truncates BookiE's text",
            case.id, case.column_type
        ));
    }
    if case.note.as_deref().is_none_or(|note| note.trim().is_empty()) {
        problems.push(format!("{}: a server_cast case needs a note saying why", case.id));
    }
    problems
}

fn corpus_marker_problems(corpus: &Corpus) -> Vec<String> {
    let mut problems = Vec::new();
    for known in &corpus.known_failing {
        let label = format!("corpus known_failing #{}", known.issue);
        if known.cases.is_empty() {
            problems.push(format!("{label}: a corpus-level marker must list its cases"));
        }
        problems.extend(known_problems(&label, known));
        for case_id in &known.cases {
            match corpus.cases.iter().find(|case| &case.id == case_id) {
                None => problems.push(format!("{label}: unknown case {case_id}")),
                Some(case) if case.skip_reason(known.route).is_some() => problems.push(format!(
                    "{label}: route {} of {case_id} is both skipped and known-failing",
                    known.route.name()
                )),
                Some(_) => {}
            }
        }
        problems.extend(
            known
                .servers
                .iter()
                .filter(|server| !corpus.servers.contains(server))
                .map(|server| format!("{label}: unknown server {server}")),
        );
    }
    problems
}

fn known_problems(label: &str, known: &KnownFailing) -> Vec<String> {
    let mut problems = Vec::new();
    if known.issue == 0 || known.reason.trim().is_empty() {
        problems.push(format!("{label}: a known failure needs an issue number and a reason"));
    }
    if known.expect.trim().is_empty() {
        problems.push(format!("{label}: a known failure needs a non-empty expect substring"));
    }
    if known.ids.iter().any(|id| *id < 1) {
        problems.push(format!("{label}: known-failing row ids start at 1"));
    }
    problems
}

fn unknown_servers(corpus: &Corpus, case: &Case) -> Vec<String> {
    let named = case
        .servers
        .iter()
        .chain(case.known_failing.iter().flat_map(|known| known.servers.iter()));
    named
        .filter(|server| !corpus.servers.contains(server))
        .map(|server| format!("{}: unknown server {server}", case.id))
        .collect()
}

fn marker_problems(case: &Case) -> Vec<String> {
    let mut problems = Vec::new();
    for known in &case.known_failing {
        problems.extend(known_problems(&case.id, known));
        if !known.cases.is_empty() {
            problems.push(format!("{}: cases belongs only on a corpus-level marker", case.id));
        }
        if known
            .ids
            .iter()
            .any(|id| usize::try_from(*id).is_ok_and(|id| id > case.literals.len()))
        {
            problems.push(format!("{}: a known-failing row id is past the last literal", case.id));
        }
        if case.skip_reason(known.route).is_some() {
            problems.push(format!(
                "{}: route {} is both skipped and known-failing",
                case.id,
                known.route.name()
            ));
        }
    }
    for skipped in &case.skip {
        if skipped.reason.trim().is_empty() {
            problems.push(format!("{}: a skipped route needs a reason", case.id));
        }
    }
    problems
}
