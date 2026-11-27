use bookie_core::{QueryResult, Value};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Bytes(Vec<u8>),
    Int(String),
    Double(String),
    Nil,
    List(Vec<Node>),
    Map(Vec<(Node, Node)>),
    Other(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scores {
    #[default]
    None,
    Single,
    WithScores,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Shape {
    pub scores: Scores,
    pub unordered: bool,
}

pub fn unescape_repr(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buffer = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            continue;
        }
        let escaped = chars.next().ok_or("dangling backslash in redis-cli output")?;
        out.push(match escaped {
            'n' => b'\n',
            'r' => b'\r',
            't' => b'\t',
            'a' => 7,
            'b' => 8,
            '\\' => b'\\',
            '"' => b'"',
            'x' => {
                let hex: String = chars.by_ref().take(2).collect();
                u8::from_str_radix(&hex, 16).map_err(|error| format!("bad \\x escape {hex:?}: {error}"))?
            }
            other => return Err(format!("unknown redis-cli escape \\{other}")),
        });
    }
    Ok(out)
}

fn quote_bare_specials(output: &str) -> String {
    let mut out = String::with_capacity(output.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut rest = output;
    while let Some(c) = rest.chars().next() {
        if !in_string
            && let Some(word) = ["-inf", "inf", "nan", "-nan"]
                .iter()
                .find(|word| rest.starts_with(**word))
        {
            out.push('"');
            out.push_str(word);
            out.push('"');
            rest = &rest[word.len()..];
            continue;
        }
        match (in_string, escaped, c) {
            (true, true, _) => escaped = false,
            (true, false, '\\') => escaped = true,
            (_, false, '"') => in_string = !in_string,
            _ => {}
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

pub fn parse_cli(output: &str) -> Result<Node, String> {
    let json: serde_json::Value = serde_json::from_str(&quote_bare_specials(output.trim()))
        .map_err(|error| format!("redis-cli output is not JSON: {error}: {output:?}"))?;
    from_cli_json(&json)
}

fn from_cli_json(json: &serde_json::Value) -> Result<Node, String> {
    Ok(match json {
        serde_json::Value::Null => Node::Nil,
        serde_json::Value::String(text) => Node::Bytes(unescape_repr(text)?),
        serde_json::Value::Number(number) => number_node(&number.to_string()),
        serde_json::Value::Bool(flag) => Node::Other(flag.to_string()),
        serde_json::Value::Array(items) => Node::List(items.iter().map(from_cli_json).collect::<Result<_, _>>()?),
        serde_json::Value::Object(map) => Node::Map(
            map.iter()
                .map(|(key, value)| Ok((Node::Bytes(unescape_repr(key)?), from_cli_json(value)?)))
                .collect::<Result<_, String>>()?,
        ),
    })
}

fn number_node(text: &str) -> Node {
    if text.contains(['.', 'e', 'E']) {
        Node::Double(text.to_string())
    } else {
        Node::Int(text.to_string())
    }
}

pub fn from_result(result: &QueryResult) -> Node {
    let names: Vec<&str> = result.columns.iter().map(|column| column.name.as_str()).collect();
    let rows: Vec<Vec<Value>> = result.rows.iter().map(<[Value]>::to_vec).collect();
    match names.as_slice() {
        ["key", "value"] => Node::Map(rows.iter().map(|row| (node_of(&row[0]), node_of(&row[1]))).collect()),
        ["value"] => Node::List(rows.iter().map(|row| node_of(&row[0])).collect()),
        _ => match rows.as_slice() {
            [row] if row.len() == 1 => node_of(&row[0]),
            _ => Node::Other(format!("unexpected result shape {names:?} with {} rows", rows.len())),
        },
    }
}

pub fn node_of(value: &Value) -> Node {
    match value {
        Value::Null => Node::Nil,
        Value::Int(number) => Node::Int(number.to_string()),
        Value::Float(number) => Node::Double(number.to_string()),
        Value::Text(text) => Node::Bytes(text.as_bytes().to_vec()),
        Value::Bytes(bytes) if std::str::from_utf8(bytes).is_ok() => {
            Node::Other(format!("valid UTF-8 carried as Bytes: {}", hex::encode(bytes)))
        }
        Value::Bytes(bytes) => Node::Bytes(bytes.clone()),
        Value::Json(json) => from_json(json),
        other => Node::Other(format!("{other:?}")),
    }
}

fn from_json(json: &serde_json::Value) -> Node {
    match json {
        serde_json::Value::Null => Node::Nil,
        serde_json::Value::String(text) => Node::Bytes(text.as_bytes().to_vec()),
        serde_json::Value::Number(number) => number_node(&number.to_string()),
        serde_json::Value::Array(items) => Node::List(items.iter().map(from_json).collect()),
        other => Node::Other(other.to_string()),
    }
}

pub fn compare(server: &Node, ours: &Node, shape: Shape) -> Vec<String> {
    let server = normalize(server);
    let ours = pair_like(&server, normalize(ours));
    let mut failures = Vec::new();
    match (&server, &ours) {
        (Node::List(theirs), Node::List(mine)) if shape.unordered => {
            let mut theirs = theirs.clone();
            let mut mine = mine.clone();
            theirs.sort_by_key(sort_key);
            mine.sort_by_key(sort_key);
            walk(&Node::List(theirs), &Node::List(mine), shape, "", &mut failures);
        }
        _ => walk(&server, &ours, shape, "", &mut failures),
    }
    failures
}

fn sort_key(node: &Node) -> String {
    format!("{node:?}")
}

fn normalize(node: &Node) -> Node {
    match node {
        Node::Map(pairs) => Node::List(
            pairs
                .iter()
                .map(|(key, value)| Node::List(vec![key.clone(), value.clone()]))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn is_pair(node: &Node) -> bool {
    matches!(node, Node::List(pair) if pair.len() == 2)
}

fn pair_like(server: &Node, ours: Node) -> Node {
    let (Node::List(theirs), Node::List(mine)) = (server, &ours) else {
        return ours;
    };
    let paired = !theirs.is_empty() && theirs.iter().all(is_pair);
    let flat = mine.iter().all(|item| !matches!(item, Node::List(_)));
    if paired && flat && mine.len() == theirs.len() * 2 {
        return Node::List(mine.chunks(2).map(|pair| Node::List(pair.to_vec())).collect());
    }
    ours
}

fn walk(server: &Node, ours: &Node, shape: Shape, path: &str, failures: &mut Vec<String>) {
    match (server, ours) {
        (Node::List(theirs), Node::List(mine)) => {
            if theirs.len() != mine.len() {
                failures.push(format!(
                    "{path}: server has {} elements, BookiE {}",
                    theirs.len(),
                    mine.len()
                ));
                return;
            }
            for (index, (theirs, mine)) in theirs.iter().zip(mine).enumerate() {
                walk(theirs, mine, shape, &format!("{path}/{index}"), failures);
            }
        }
        _ if scalar_matches(server, ours, shape, path) => {}
        _ => failures.push(format!(
            "{path}: server {}, BookiE {}",
            describe(server),
            describe(ours)
        )),
    }
}

fn scalar_matches(server: &Node, ours: &Node, shape: Shape, path: &str) -> bool {
    if server == ours {
        return true;
    }
    let at_score = match shape.scores {
        Scores::None => false,
        Scores::Single => path.is_empty(),
        Scores::WithScores => path.ends_with("/1"),
    };
    at_score
        && score_value(server)
            .zip(score_value(ours))
            .is_some_and(|(a, b)| a.to_bits() == b.to_bits())
}

fn score_value(node: &Node) -> Option<f64> {
    match node {
        Node::Double(text) | Node::Int(text) => text.parse().ok(),
        Node::Bytes(bytes) => std::str::from_utf8(bytes).ok()?.parse().ok(),
        _ => None,
    }
}

pub fn describe(node: &Node) -> String {
    match node {
        Node::Bytes(bytes) if bytes.len() > 60 => format!("Bytes({} bytes)", bytes.len()),
        Node::Bytes(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => format!("{text:?}"),
            Err(_) => format!("0x{}", hex::encode(bytes)),
        },
        Node::List(items) => format!("list of {}", items.len()),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
#[path = "redis_reply_tests.rs"]
mod tests;
