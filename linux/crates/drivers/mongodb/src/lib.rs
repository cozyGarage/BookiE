use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::{ClientOptions, Tls, TlsOptions};
use mongodb::{Client, Database};
use secrecy::ExposeSecret;

use tablepro_core::{
    ColumnInfo, ConnectOptions, Connection, DatabaseDriver, DriverError, DriverMaturity, ExecResult, MAX_QUERY_ROWS,
    QueryResult, TableInfo, Value, looks_like_tls_failure,
};

const SAMPLE_DOCS: i64 = 50;

pub struct MongodbDriver;

#[async_trait]
impl DatabaseDriver for MongodbDriver {
    fn id(&self) -> &'static str {
        "mongodb"
    }

    fn display_name(&self) -> &'static str {
        "MongoDB"
    }

    fn maturity(&self) -> DriverMaturity {
        DriverMaturity::Experimental
    }

    fn default_port(&self) -> u16 {
        27017
    }

    async fn connect(&self, opts: ConnectOptions) -> Result<Box<dyn Connection>, DriverError> {
        let verifies_cert = opts.tls.mode.verifies_cert();
        let client_opts = build_client_options(&opts).await?;
        let client = Client::with_options(client_opts).map_err(|err| map_mongo_connect_error(err, verifies_cert))?;
        let database_name = if opts.database.is_empty() {
            "test".into()
        } else {
            opts.database
        };
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|err| map_mongo_connect_error(err, verifies_cert))?;
        Ok(Box::new(MongodbConnection { client, database_name }))
    }
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

async fn build_client_options(opts: &ConnectOptions) -> Result<ClientOptions, DriverError> {
    let scheme = "mongodb";
    let password = opts.password.expose_secret();
    let auth = if !opts.username.is_empty() {
        format!("{}:{}@", encode_uri(&opts.username), encode_uri(password))
    } else {
        String::new()
    };
    let db_path = if opts.database.is_empty() {
        String::new()
    } else {
        format!("/{}", encode_uri(&opts.database))
    };
    let uri = format!("{scheme}://{auth}{}:{}{db_path}", opts.host, opts.port);
    let mut client_opts = ClientOptions::parse(&uri).await.map_err(map_mongo_error)?;
    client_opts.app_name = Some("TablePro".into());
    client_opts.tls = Some(tls_for(&opts.tls));
    client_opts.connect_timeout = Some(CONNECT_TIMEOUT);
    client_opts.server_selection_timeout = Some(CONNECT_TIMEOUT);
    // Every saved connection names exactly one host. Without this, SDAM
    // topology discovery can pick up a replica set member's advertised
    // hostname and try to redial it directly, bypassing the SSH tunnel
    // or reaching a host the client can't resolve.
    client_opts.direct_connection = Some(true);
    Ok(client_opts)
}

/// Map the shared TLS modes onto what the rustls-backed MongoDB driver can
/// express. It has no CA-only mode, so `VerifyCa` verifies the hostname too,
/// which is stricter than requested and never weaker.
fn tls_for(config: &tablepro_core::TlsConfig) -> Tls {
    use tablepro_core::TlsMode;
    if config.mode == TlsMode::Disabled {
        return Tls::Disabled;
    }
    let mut options = TlsOptions::default();
    if let Some(path) = &config.root_cert {
        options.ca_file_path = Some(path.clone());
    }
    if !config.mode.verifies_cert() {
        options.allow_invalid_certificates = Some(true);
    }
    Tls::Enabled(options)
}

struct MongodbConnection {
    client: Client,
    database_name: String,
}

impl MongodbConnection {
    fn db(&self) -> Database {
        self.client.database(&self.database_name)
    }
}

#[async_trait]
impl Connection for MongodbConnection {
    async fn list_tables(&self) -> Result<Vec<TableInfo>, DriverError> {
        let names = self.db().list_collection_names().await.map_err(map_mongo_error)?;
        let mut tables: Vec<TableInfo> = names
            .into_iter()
            .map(|name| TableInfo {
                schema: Some(self.database_name.clone()),
                name,
            })
            .collect();
        tables.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tables)
    }

    async fn fetch_columns(&self, _schema: Option<&str>, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        let coll = self.db().collection::<Document>(table);
        let mut cursor = coll.find(doc! {}).limit(SAMPLE_DOCS).await.map_err(map_mongo_error)?;
        let mut union: BTreeMap<String, String> = BTreeMap::new();
        while let Some(doc) = cursor.try_next().await.map_err(map_mongo_error)? {
            for (key, value) in doc {
                union.entry(key).or_insert_with(|| bson_type_name(&value));
            }
        }
        if !union.contains_key("_id") {
            union.insert("_id".into(), "ObjectId".into());
        }
        let mut columns: Vec<ColumnInfo> = Vec::with_capacity(union.len());
        if let Some(ty) = union.remove("_id") {
            columns.push(ColumnInfo {
                name: "_id".into(),
                data_type: ty,
                nullable: false,
                primary_key: true,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
            });
        }
        for (name, data_type) in union {
            columns.push(ColumnInfo {
                name,
                data_type,
                nullable: true,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
            });
        }
        Ok(columns)
    }

    async fn fetch_rows(
        &self,
        _schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        let columns = self.fetch_columns(None, table).await?;
        let coll = self.db().collection::<Document>(table);
        let mut cursor = coll
            .find(doc! {})
            .skip(offset)
            .limit(limit as i64)
            .await
            .map_err(map_mongo_error)?;
        let mut rows = Vec::new();
        while let Some(doc) = cursor.try_next().await.map_err(map_mongo_error)? {
            rows.push(document_to_row(&doc, &columns));
        }
        Ok(QueryResult {
            columns,
            rows,
            truncated: false,
        })
    }

    async fn query(&self, sql: &str) -> Result<QueryResult, DriverError> {
        let trimmed = sql.trim();
        if let Some(parsed) = parse_find_shell(trimmed) {
            return self.run_find(parsed).await;
        }
        if let Some(parsed) = parse_aggregate_shell(trimmed) {
            return self.run_aggregate(parsed).await;
        }
        if trimmed.starts_with('{') {
            let filter: Document = mongodb::bson::from_slice(trimmed.as_bytes())
                .or_else(|_| serde_json_to_document(trimmed))
                .map_err(|e| DriverError::Query {
                    message: format!("invalid MongoDB filter JSON: {e}"),
                    sqlstate: None,
                })?;
            let coll_name = self
                .list_tables()
                .await?
                .into_iter()
                .next()
                .map(|t| t.name)
                .ok_or_else(|| DriverError::Query {
                    message: "no collections available for filter query".into(),
                    sqlstate: None,
                })?;
            return self
                .run_find(FindQuery {
                    collection: coll_name,
                    filter,
                    skip: 0,
                    limit: MAX_QUERY_ROWS as i64,
                })
                .await;
        }
        Err(DriverError::Unsupported(format!(
            "MongoDB driver accepts db.collection.find(...) / aggregate(...) or JSON filter; got: {trimmed}"
        )))
    }

    async fn execute(&self, sql: &str) -> Result<ExecResult, DriverError> {
        let trimmed = sql.trim();
        if let Some((coll, doc)) = parse_insert_one(trimmed) {
            self.db()
                .collection::<Document>(&coll)
                .insert_one(doc)
                .await
                .map_err(map_mongo_error)?;
            return Ok(ExecResult { rows_affected: 1 });
        }
        if let Some((coll, filter)) = parse_delete_many(trimmed) {
            let result = self
                .db()
                .collection::<Document>(&coll)
                .delete_many(filter)
                .await
                .map_err(map_mongo_error)?;
            return Ok(ExecResult {
                rows_affected: result.deleted_count,
            });
        }
        if let Some(coll) = parse_drop_table_sql(trimmed) {
            self.db()
                .collection::<Document>(&coll)
                .drop()
                .await
                .map_err(map_mongo_error)?;
            return Ok(ExecResult { rows_affected: 0 });
        }
        Err(DriverError::Unsupported(
            "MongoDB execute supports insertOne/deleteMany shell forms and DROP TABLE only in this MVP".into(),
        ))
    }

    async fn execute_params(&self, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError> {
        if params.is_empty() {
            return self.execute(sql).await;
        }
        Err(DriverError::Unsupported(
            "MongoDB execute_params does not support bound parameters".into(),
        ))
    }

    async fn execute_in_transaction(&self, statements: &[(String, Vec<Value>)]) -> Result<Vec<u64>, DriverError> {
        let mut affected = Vec::with_capacity(statements.len());
        for (idx, (sql, params)) in statements.iter().enumerate() {
            match self.execute_params(sql, params).await {
                Ok(res) => affected.push(res.rows_affected),
                Err(e) => {
                    return Err(DriverError::Transaction {
                        statement_index: idx,
                        source: Box::new(e),
                    });
                }
            }
        }
        Ok(affected)
    }

    async fn ping(&self) -> Result<(), DriverError> {
        self.client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(map_mongo_error)?;
        Ok(())
    }

    async fn server_version(&self) -> Result<Option<String>, DriverError> {
        let reply = self
            .client
            .database("admin")
            .run_command(doc! { "buildInfo": 1 })
            .await
            .map_err(map_mongo_error)?;
        let version = reply.get_str("version").ok().map(|v| format!("MongoDB {v}"));
        Ok(version)
    }

    async fn close(self: Box<Self>) -> Result<(), DriverError> {
        Ok(())
    }
}

struct FindQuery {
    collection: String,
    filter: Document,
    skip: u64,
    limit: i64,
}

struct AggregateQuery {
    collection: String,
    pipeline: Vec<Document>,
}

impl MongodbConnection {
    async fn run_find(&self, q: FindQuery) -> Result<QueryResult, DriverError> {
        let columns = self.fetch_columns(None, &q.collection).await?;
        let coll = self.db().collection::<Document>(&q.collection);
        let mut cursor = coll
            .find(q.filter)
            .skip(q.skip)
            .limit(q.limit)
            .await
            .map_err(map_mongo_error)?;
        let mut rows = Vec::new();
        let mut truncated = false;
        while let Some(doc) = cursor.try_next().await.map_err(map_mongo_error)? {
            if rows.len() >= MAX_QUERY_ROWS {
                truncated = true;
                break;
            }
            rows.push(document_to_row(&doc, &columns));
        }
        Ok(QueryResult {
            columns,
            rows,
            truncated,
        })
    }

    async fn run_aggregate(&self, q: AggregateQuery) -> Result<QueryResult, DriverError> {
        let coll = self.db().collection::<Document>(&q.collection);
        let mut cursor = coll.aggregate(q.pipeline).await.map_err(map_mongo_error)?;
        let mut docs = Vec::new();
        let mut truncated = false;
        while let Some(doc) = cursor.try_next().await.map_err(map_mongo_error)? {
            if docs.len() >= MAX_QUERY_ROWS {
                truncated = true;
                break;
            }
            docs.push(doc);
        }
        let columns = columns_from_docs(&docs);
        let rows = docs.iter().map(|d| document_to_row(d, &columns)).collect();
        Ok(QueryResult {
            columns,
            rows,
            truncated,
        })
    }
}

fn columns_from_docs(docs: &[Document]) -> Vec<ColumnInfo> {
    let mut union: BTreeMap<String, String> = BTreeMap::new();
    for doc in docs {
        for (key, value) in doc {
            union.entry(key.clone()).or_insert_with(|| bson_type_name(value));
        }
    }
    let mut columns = Vec::new();
    if let Some(ty) = union.remove("_id") {
        columns.push(ColumnInfo {
            name: "_id".into(),
            data_type: ty,
            nullable: false,
            primary_key: true,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        });
    }
    for (name, data_type) in union {
        columns.push(ColumnInfo {
            name,
            data_type,
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        });
    }
    columns
}

fn document_to_row(doc: &Document, columns: &[ColumnInfo]) -> Vec<Value> {
    columns
        .iter()
        .map(|c| match doc.get(&c.name) {
            Some(b) => bson_to_value(b),
            None => Value::Null,
        })
        .collect()
}

fn bson_to_value(b: &Bson) -> Value {
    match b {
        Bson::Null => Value::Null,
        Bson::Boolean(v) => Value::Bool(*v),
        Bson::Int32(v) => Value::Int(*v as i64),
        Bson::Int64(v) => Value::Int(*v),
        Bson::Double(v) => Value::Float(*v),
        Bson::String(v) => Value::Text(v.clone()),
        Bson::ObjectId(v) => Value::Text(v.to_hex()),
        Bson::DateTime(v) => Value::Text(v.try_to_rfc3339_string().unwrap_or_else(|_| v.to_string())),
        Bson::Binary(bin) => Value::Bytes(bin.bytes.clone()),
        Bson::Decimal128(d) => Value::Text(d.to_string()),
        Bson::Document(d) => Value::Json(document_to_json(d)),
        Bson::Array(a) => Value::Json(serde_json::Value::Array(a.iter().map(bson_to_json).collect())),
        other => Value::Text(other.to_string()),
    }
}

fn bson_to_json(b: &Bson) -> serde_json::Value {
    match b {
        Bson::Null => serde_json::Value::Null,
        Bson::Boolean(v) => serde_json::Value::Bool(*v),
        Bson::Int32(v) => serde_json::json!(*v),
        Bson::Int64(v) => serde_json::json!(*v),
        Bson::Double(v) => serde_json::json!(*v),
        Bson::String(v) => serde_json::Value::String(v.clone()),
        Bson::Document(d) => document_to_json(d),
        Bson::Array(a) => serde_json::Value::Array(a.iter().map(bson_to_json).collect()),
        other => serde_json::Value::String(other.to_string()),
    }
}

fn document_to_json(doc: &Document) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, v) in doc {
        map.insert(k.clone(), bson_to_json(v));
    }
    serde_json::Value::Object(map)
}

fn bson_type_name(b: &Bson) -> String {
    match b {
        Bson::Null => "null",
        Bson::Boolean(_) => "bool",
        Bson::Int32(_) => "int",
        Bson::Int64(_) => "long",
        Bson::Double(_) => "double",
        Bson::String(_) => "string",
        Bson::ObjectId(_) => "ObjectId",
        Bson::DateTime(_) => "date",
        Bson::Binary(_) => "binData",
        Bson::Document(_) => "object",
        Bson::Array(_) => "array",
        Bson::Decimal128(_) => "decimal",
        _ => "mixed",
    }
    .into()
}

fn encode_uri(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Parse `db.coll.find({...})` / `db["coll"].find({...}).skip(n).limit(m)`.
fn parse_find_shell(input: &str) -> Option<FindQuery> {
    let input = input.trim().trim_end_matches(';');
    let (collection, rest) = split_collection_call(input, "find")?;
    let (filter_src, after_filter) = extract_balanced(rest.trim_start(), '(', ')')?;
    let filter = if filter_src.trim().is_empty() {
        Document::new()
    } else {
        serde_json_to_document(filter_src).ok()?
    };
    let mut skip = 0u64;
    let mut limit = MAX_QUERY_ROWS as i64;
    let mut remaining = after_filter;
    while let Some(pos) = remaining.find('.') {
        remaining = &remaining[pos + 1..];
        if let Some(rest) = remaining.strip_prefix("skip") {
            let (n, after) = extract_balanced(rest.trim_start(), '(', ')')?;
            skip = n.trim().parse().ok()?;
            remaining = after;
        } else if let Some(rest) = remaining.strip_prefix("limit") {
            let (n, after) = extract_balanced(rest.trim_start(), '(', ')')?;
            limit = n.trim().parse().ok()?;
            remaining = after;
        } else {
            break;
        }
    }
    if !remaining.trim().is_empty() {
        return None;
    }
    Some(FindQuery {
        collection,
        filter,
        skip,
        limit,
    })
}

fn parse_aggregate_shell(input: &str) -> Option<AggregateQuery> {
    let input = input.trim().trim_end_matches(';');
    let (collection, rest) = split_collection_call(input, "aggregate")?;
    let (pipeline_src, trailing) = extract_balanced(rest.trim_start(), '(', ')')?;
    if !trailing.trim().is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(pipeline_src).ok()?;
    let arr = value.as_array()?;
    let mut pipeline = Vec::new();
    for item in arr {
        pipeline.push(serde_json_to_document(&item.to_string()).ok()?);
    }
    Some(AggregateQuery { collection, pipeline })
}

fn parse_insert_one(input: &str) -> Option<(String, Document)> {
    let (collection, rest) = split_collection_call(input.trim().trim_end_matches(';'), "insertOne")?;
    let (doc_src, trailing) = extract_balanced(rest.trim_start(), '(', ')')?;
    if !trailing.trim().is_empty() {
        return None;
    }
    Some((collection, serde_json_to_document(doc_src).ok()?))
}

fn parse_delete_many(input: &str) -> Option<(String, Document)> {
    let (collection, rest) = split_collection_call(input.trim().trim_end_matches(';'), "deleteMany")?;
    let (doc_src, trailing) = extract_balanced(rest.trim_start(), '(', ')')?;
    if !trailing.trim().is_empty() {
        return None;
    }
    Some((collection, serde_json_to_document(doc_src).ok()?))
}

/// The Structure tab's "Drop table" action builds engine-neutral SQL
/// through `tablepro_core::sql_ddl::build_drop_table`, which has no
/// MongoDB-specific case -- it emits `DROP TABLE IF EXISTS "name"`
/// with the same quoting every other driver gets. Recognize that shape
/// here and translate it into a native collection drop instead of
/// failing the drop outright.
fn parse_drop_table_sql(input: &str) -> Option<String> {
    let trimmed = input.trim().trim_end_matches(';').trim();
    let rest = trimmed.strip_prefix("DROP TABLE")?.trim_start();
    let rest = rest.strip_prefix("IF EXISTS").unwrap_or(rest).trim_start();
    let Some(quoted) = rest.strip_prefix('"') else {
        return is_simple_ident(rest).then(|| rest.to_owned());
    };
    let mut chars = quoted.chars();
    let mut name = String::new();
    loop {
        match chars.next()? {
            '"' if chars.clone().next() == Some('"') => {
                chars.next();
                name.push('"');
            }
            '"' => break,
            c => name.push(c),
        }
    }
    chars.as_str().trim().is_empty().then_some(name)
}

fn split_collection_call<'a>(input: &'a str, method: &str) -> Option<(String, &'a str)> {
    let rest = input.strip_prefix("db")?;
    let (collection, after) = if let Some(rest) = rest.strip_prefix('.') {
        let end = rest.find('.')?;
        let name = &rest[..end];
        if !is_simple_ident(name) {
            return None;
        }
        (name.to_string(), &rest[end..])
    } else if let Some(rest) = rest.strip_prefix("[\"") {
        let end = rest.find("\"]")?;
        let name = rest[..end].to_string();
        let after = &rest[end + 2..];
        (name, after)
    } else {
        let rest = rest.strip_prefix("['")?;
        let end = rest.find("']")?;
        let name = rest[..end].to_string();
        let after = &rest[end + 2..];
        (name, after)
    };
    let after = after.strip_prefix('.')?;
    let after = after.strip_prefix(method)?;
    Some((collection, after))
}

fn extract_balanced(input: &str, open: char, close: char) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if !input.starts_with(open) {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = None::<char>;
    let mut escaped = false;
    for (i, c) in input.char_indices() {
        if let Some(q) = in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                in_string = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => in_string = Some(c),
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some((&input[1..i], &input[i + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

fn is_simple_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn serde_json_to_document(src: &str) -> Result<Document, String> {
    let value: serde_json::Value = serde_json::from_str(src).map_err(|e| format!("JSON parse error: {e}"))?;
    match json_to_bson(value)? {
        Bson::Document(document) => Ok(document),
        _ => Err("expected a JSON object".into()),
    }
}

// Serializing serde_json::Value through BSON exposes serde_json's private
// arbitrary-precision number representation as a document. Convert values
// explicitly so numeric predicates and inserted numbers stay BSON numbers.
fn json_to_bson(value: serde_json::Value) -> Result<Bson, String> {
    Ok(match value {
        serde_json::Value::Null => Bson::Null,
        serde_json::Value::Bool(value) => Bson::Boolean(value),
        serde_json::Value::String(value) => Bson::String(value),
        serde_json::Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                Bson::Int64(integer)
            } else if value.as_u64().is_some() {
                Bson::Decimal128(
                    value
                        .to_string()
                        .parse()
                        .map_err(|error| format!("BSON number: {error}"))?,
                )
            } else {
                Bson::Double(
                    value
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or("BSON number out of range")?,
                )
            }
        }
        serde_json::Value::Array(values) => {
            Bson::Array(values.into_iter().map(json_to_bson).collect::<Result<_, _>>()?)
        }
        serde_json::Value::Object(values) => Bson::Document(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, json_to_bson(value)?)))
                .collect::<Result<_, String>>()?,
        ),
    })
}

/// The mongodb crate boxes the transport error inside `ErrorKind::Io`
/// rather than exposing it through `source()`, so the handshake cause is
/// only reachable by walking that box as a second chain.
fn error_chain_text(err: &mongodb::error::Error) -> String {
    let mut text = tablepro_core::error_chain_text(err);
    if let mongodb::error::ErrorKind::Io(io) = &*err.kind {
        text.push(' ');
        text.push_str(&tablepro_core::error_chain_text(io.as_ref()));
    }
    text
}

fn mongo_error_can_hide_tls(kind: &mongodb::error::ErrorKind) -> bool {
    use mongodb::error::ErrorKind;
    matches!(
        kind,
        ErrorKind::Io(_)
            | ErrorKind::ServerSelection { .. }
            | ErrorKind::DnsResolve { .. }
            | ErrorKind::ConnectionPoolCleared { .. }
    )
}

fn map_mongo_error(err: mongodb::error::Error) -> DriverError {
    map_mongo_connect_error(err, false)
}

fn map_mongo_connect_error(err: mongodb::error::Error, _verifies_cert: bool) -> DriverError {
    use mongodb::error::ErrorKind;
    let chain = error_chain_text(&err);
    if mongo_error_can_hide_tls(&err.kind) && looks_like_tls_failure(&chain) {
        return DriverError::Tls(chain);
    }
    match &*err.kind {
        ErrorKind::Authentication { .. } => DriverError::AuthFailed,
        ErrorKind::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused => DriverError::ConnectionRefused,
        ErrorKind::ServerSelection { .. } | ErrorKind::DnsResolve { .. } => DriverError::ConnectionRefused,
        _ => DriverError::Query {
            message: err.to_string(),
            sqlstate: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_metadata_is_not_declared_without_a_fetch() {
        let d = MongodbDriver;
        let source = include_str!("lib.rs");
        assert!(!d.supports_index_metadata());
        assert!(!d.supports_foreign_key_metadata());
        assert!(!source.contains(&["async fn ", "fetch_indexes("].concat()));
        assert!(!source.contains(&["async fn ", "fetch_foreign_keys("].concat()));
    }

    #[test]
    fn driver_metadata() {
        let d = MongodbDriver;
        assert_eq!(d.id(), "mongodb");
        assert_eq!(d.display_name(), "MongoDB");
        assert_eq!(d.default_port(), 27017);
    }

    #[test]
    fn patched_mongodb_caps_scram_iterations() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../vendor/mongodb/src/client/auth/scram.rs"
        ));
        assert!(source.contains("const MAX_ITERATION_COUNT: u32 = 100_000"));
        assert!(source.contains("self.i > MAX_ITERATION_COUNT"));
        assert!(source.contains("self.nonce.starts_with(nonce)"));
    }

    #[test]
    fn drop_table_sql_extracts_the_collection_name() {
        assert_eq!(
            parse_drop_table_sql(r#"DROP TABLE IF EXISTS "widgets""#),
            Some("widgets".to_string())
        );
        assert_eq!(
            parse_drop_table_sql(r#"DROP TABLE "widgets""#),
            Some("widgets".to_string())
        );
    }

    #[test]
    fn drop_table_sql_unescapes_a_doubled_quote_in_the_name() {
        assert_eq!(
            parse_drop_table_sql(r#"DROP TABLE IF EXISTS "a""b""#),
            Some(r#"a"b"#.to_string())
        );
    }

    #[test]
    fn drop_table_sql_ignores_an_unrelated_statement() {
        assert_eq!(parse_drop_table_sql("SELECT 1"), None);
        assert_eq!(parse_drop_table_sql("DROP TABLE widgets"), Some("widgets".into()));
        assert_eq!(
            parse_drop_table_sql("DROP TABLE IF EXISTS widgets"),
            Some("widgets".into())
        );
        assert_eq!(parse_drop_table_sql("DROP TABLE widgets extra"), None);
        assert_eq!(parse_drop_table_sql("DROP TABLE wid-gets"), None);
        assert_eq!(parse_drop_table_sql("DROP TABLE "), None);
        assert_eq!(parse_drop_table_sql(r#"db.widgets.deleteMany({})"#), None);
    }

    #[tokio::test]
    async fn a_single_host_connection_always_requests_a_direct_connection() {
        let opts = ConnectOptions {
            host: "db.internal".into(),
            port: 27017,
            ..Default::default()
        };
        let client_opts = build_client_options(&opts).await.expect("build client options");
        assert_eq!(client_opts.direct_connection, Some(true));
    }

    #[test]
    fn a_connection_refused_io_error_maps_to_connection_refused() {
        let io = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        let error = mongodb::error::Error::from(io);
        assert!(matches!(map_mongo_error(error), DriverError::ConnectionRefused));
    }

    #[test]
    fn an_unrelated_io_error_is_not_mistaken_for_connection_refused() {
        let io = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let error = mongodb::error::Error::from(io);
        assert!(matches!(map_mongo_error(error), DriverError::Query { .. }));
    }

    /// The mongodb error's own Display prints only the io error's message,
    /// not the chain behind it, so a handshake cause one layer deeper is
    /// lost unless the Io box is walked as its own chain.
    #[test]
    fn a_handshake_cause_nested_under_the_io_error_still_reaches_the_text() {
        #[derive(Debug)]
        struct Handshake;

        impl std::fmt::Display for Handshake {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("invalid peer certificate: NotValidForName")
            }
        }

        impl std::error::Error for Handshake {}

        #[derive(Debug)]
        struct Transport(Handshake);

        impl std::fmt::Display for Transport {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("transport closed")
            }
        }

        impl std::error::Error for Transport {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let io = std::io::Error::other(Transport(Handshake));
        let err = mongodb::error::Error::from(io);

        let text = error_chain_text(&err);
        assert!(
            looks_like_tls_failure(&text),
            "the nested handshake cause must survive into the chain: {text}"
        );
        assert!(
            !looks_like_tls_failure(&tablepro_core::error_chain_text(&err)),
            "this case is only reachable by walking the Io box"
        );
    }

    #[test]
    fn a_certificate_name_mismatch_io_error_maps_to_tls() {
        let io = std::io::Error::other("invalid peer certificate: certificate not valid for name \"127.0.0.1\"");
        let mapped = map_mongo_error(mongodb::error::Error::from(io));
        assert!(matches!(mapped, DriverError::Tls(detail) if detail.contains("certificate")));
    }

    #[test]
    fn a_connection_refused_carrying_a_name_mismatch_maps_to_tls() {
        let io = std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "invalid peer certificate: certificate not valid for name \"127.0.0.1\"",
        );
        let mapped = map_mongo_error(mongodb::error::Error::from(io));
        assert!(matches!(mapped, DriverError::Tls(detail) if detail.contains("certificate")));
    }

    #[test]
    fn a_plain_connection_refused_stays_connection_refused_when_verifying() {
        let io = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        let mapped = map_mongo_connect_error(mongodb::error::Error::from(io), true);
        assert!(matches!(mapped, DriverError::ConnectionRefused));
    }

    #[test]
    fn verifying_connect_does_not_report_a_hostname_mismatch_as_a_refusal() {
        let io = std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "invalid peer certificate: NotValidForName",
        );
        let mapped = map_mongo_connect_error(mongodb::error::Error::from(io), true);
        assert!(matches!(
            mapped,
            DriverError::Tls(detail)
                if detail.to_ascii_lowercase().contains("certificate")
                    || detail.to_ascii_lowercase().contains("notvalidforname")
        ));
    }

    #[test]
    fn a_server_selection_timeout_embedding_a_name_mismatch_is_tls() {
        let text = "Server selection timeout: No available servers. Topology: { Type: Unknown, \
                    Servers: [ { Address: 127.0.0.1:27018, Type: Unknown, Error: Kind: I/O error: \
                    invalid peer certificate: certificate not valid for name \"127.0.0.1\" } ] }";
        assert!(looks_like_tls_failure(text));
        assert!(!looks_like_tls_failure(
            "Server selection timeout: No available servers. Topology: { Type: Unknown }"
        ));
    }

    #[test]
    fn parse_find_shell_basic() {
        let q = parse_find_shell(r#"db.users.find({"age": {"$gt": 18}}).limit(10)"#).unwrap();
        assert_eq!(q.collection, "users");
        assert_eq!(q.limit, 10);
        assert_eq!(q.filter.get_document("age").unwrap().get_i64("$gt").unwrap(), 18);
    }

    #[test]
    fn arbitrary_precision_json_numbers_remain_bson_numbers() {
        let document = serde_json_to_document(
            r#"{"nested":{"n":9223372036854775807},"values":[18,1.25],"wide":18446744073709551615}"#,
        )
        .unwrap();
        assert_eq!(document.get_document("nested").unwrap().get_i64("n").unwrap(), i64::MAX);
        assert_eq!(
            document.get_array("values").unwrap(),
            &vec![Bson::Int64(18), Bson::Double(1.25)]
        );
        assert!(matches!(document.get("wide"), Some(Bson::Decimal128(_))));
        assert!(serde_json_to_document("[]").is_err());
    }

    #[test]
    fn parse_find_shell_bracket_name() {
        let q = parse_find_shell(r#"db["my-coll"].find({})"#).unwrap();
        assert_eq!(q.collection, "my-coll");
    }

    #[test]
    fn shell_commands_reject_trailing_text_instead_of_executing_a_prefix() {
        assert!(parse_find_shell(r#"db.users.find({}).limit(1) unexpected"#).is_none());
        assert!(parse_find_shell(r#"db.users.find({}).unknown()"#).is_none());
        assert!(parse_aggregate_shell(r#"db.users.aggregate([]) unexpected"#).is_none());
        assert!(parse_insert_one(r#"db.users.insertOne({"name":"Ada"}) unexpected"#).is_none());
        assert!(parse_delete_many(r#"db.users.deleteMany({}) unexpected"#).is_none());
        assert!(parse_drop_table_sql(r#"DROP TABLE "users" unexpected"#).is_none());
    }

    #[test]
    fn parse_aggregate_shell_pipeline() {
        let q = parse_aggregate_shell(r#"db.orders.aggregate([{"$match": {"status": "a"}}])"#).unwrap();
        assert_eq!(q.collection, "orders");
        assert_eq!(q.pipeline.len(), 1);
    }

    #[test]
    fn bson_type_name_object_id() {
        assert_eq!(bson_type_name(&Bson::Null), "null");
        assert_eq!(bson_type_name(&Bson::String("x".into())), "string");
    }
}
