use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

mod codec;
mod error;
mod shell;
mod update;

use async_trait::async_trait;
use codec::{
    bson_to_value, columns_from_docs, columns_from_types, document_to_row, merge_page_types, observe_bson_type,
    serde_json_to_document,
};
use error::{map_mongo_connect_error, map_mongo_error};
use futures::TryStreamExt;
use mongodb::bson::{Document, doc};
use mongodb::options::{ClientOptions, Tls, TlsOptions};
use mongodb::{Client, Database};
use secrecy::ExposeSecret;
use shell::{
    AggregateQuery, FindQuery, parse_aggregate_shell, parse_delete_many, parse_drop_table_sql, parse_find_shell,
    parse_insert_one,
};
use update::{parse_keyed_delete, parse_keyed_update, parse_keyed_value_select};

use tablepro_core::{
    ColumnInfo, ConnectOptions, Connection, DatabaseDriver, DriverError, DriverMaturity, ExecResult,
    MAX_QUERY_RESULT_BYTES, MAX_QUERY_RESULT_CELLS, MAX_QUERY_ROWS, QueryResult, QueryResultBudget, TableInfo, Value,
};

pub struct MongodbDriver;

/// Validate text using BSON Decimal128's range and return its exact canonical text.
/// The app uses this when Rust's narrower `Decimal` parser cannot represent an edit.
pub fn canonical_decimal128_text(text: &str) -> Result<String, DriverError> {
    text.parse::<mongodb::bson::Decimal128>()
        .map(|value| value.to_string())
        .map_err(|error| DriverError::Unsupported(format!("invalid MongoDB Decimal128 value: {error}")))
}

#[async_trait]
impl DatabaseDriver for MongodbDriver {
    fn supports_database_listing(&self) -> bool {
        true
    }

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

    fn default_database(&self) -> &'static str {
        "test"
    }

    fn default_username(&self) -> &'static str {
        ""
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
        Ok(Box::new(MongodbConnection::new(client, database_name)))
    }
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

async fn build_client_options(opts: &ConnectOptions) -> Result<ClientOptions, DriverError> {
    let scheme = "mongodb";
    let host = mongo_host_authority(&opts.host)?;
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
    let uri = format!("{scheme}://{auth}{host}:{}{db_path}", opts.port);
    let mut client_opts = ClientOptions::parse(&uri).await.map_err(map_mongo_error)?;
    client_opts.app_name = Some("TablePro".into());
    client_opts.tls = Some(tls_for(&opts.tls, opts.service_address().0));
    client_opts.connect_timeout = Some(CONNECT_TIMEOUT);
    client_opts.server_selection_timeout = Some(CONNECT_TIMEOUT);
    // Every saved connection names exactly one host. Without this, SDAM
    // topology discovery can pick up a replica set member's advertised
    // hostname and try to redial it directly, bypassing the SSH tunnel
    // or reaching a host the client can't resolve.
    client_opts.direct_connection = Some(true);
    Ok(client_opts)
}

fn mongo_host_authority(host: &str) -> Result<String, DriverError> {
    let bracketed = host.starts_with('[') || host.ends_with(']');
    let ip_host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(address) = ip_host.parse::<std::net::IpAddr>() {
        return Ok(match address {
            std::net::IpAddr::V4(address) if !bracketed => address.to_string(),
            std::net::IpAddr::V4(_) => return Err(DriverError::Unsupported("invalid MongoDB host".into())),
            std::net::IpAddr::V6(address) => format!("[{address}]"),
        });
    }
    let labels = host.strip_suffix('.').unwrap_or(host).split('.');
    let valid = !host.is_empty()
        && host.len() <= 254
        && labels.into_iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        });
    if valid {
        Ok(host.to_string())
    } else {
        Err(DriverError::Unsupported("invalid MongoDB host".into()))
    }
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

/// Map the shared TLS modes onto what the rustls-backed MongoDB driver can
/// express. It has no CA-only mode, so `VerifyCa` verifies the hostname too,
/// which is stricter than requested and never weaker.
///
/// `service_host` is the database's own hostname, which is what the
/// certificate was issued for. The dial address can instead be an SSH
/// tunnel's `127.0.0.1`, so the verifier is pinned to `service_host` rather
/// than left to derive a name from the dial address.
fn tls_for(config: &tablepro_core::TlsConfig, service_host: &str) -> Tls {
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
    options.verify_hostname = Some(service_host.to_string());
    Tls::Enabled(options)
}

struct MongodbConnection {
    client: Client,
    database_name: String,
    columns: tokio::sync::RwLock<HashMap<String, Vec<ColumnInfo>>>,
}

fn bounded_document_rows(docs: &[Document], columns: &[ColumnInfo]) -> (Vec<Vec<Value>>, bool) {
    let mut rows = Vec::new();
    let mut budget = QueryResultBudget::default();
    for doc in docs {
        let row = document_to_row(doc, columns);
        if !budget.admit(&row) {
            return (rows, true);
        }
        rows.push(row);
    }
    (rows, false)
}

impl MongodbConnection {
    fn new(client: Client, database_name: String) -> Self {
        Self {
            client,
            database_name,
            columns: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    fn db(&self) -> Database {
        self.client.database(&self.database_name)
    }

    async fn census_columns(&self, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        let coll = self.db().collection::<Document>(table);
        let mut cursor = coll.find(doc! {}).await.map_err(map_mongo_error)?;
        let mut types = BTreeMap::new();
        while let Some(doc) = cursor.try_next().await.map_err(map_mongo_error)? {
            for (key, value) in &doc {
                observe_bson_type(&mut types, key, value);
            }
        }
        if !types.contains_key("_id") {
            types.insert("_id".into(), "ObjectId".into());
        }
        Ok(columns_from_types(types))
    }

    async fn page_columns(&self, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        if let Some(columns) = self.columns.read().await.get(table).cloned() {
            return Ok(columns);
        }
        let mut cache = self.columns.write().await;
        if let Some(columns) = cache.get(table) {
            return Ok(columns.clone());
        }
        let columns = self.census_columns(table).await?;
        cache.insert(table.into(), columns.clone());
        Ok(columns)
    }

    async fn invalidate_columns(&self, table: &str) {
        self.columns.write().await.remove(table);
    }
}

#[async_trait]
impl Connection for MongodbConnection {
    async fn list_databases(&self) -> Result<Vec<String>, DriverError> {
        let mut names = self.client.list_database_names().await.map_err(map_mongo_error)?;
        names.retain(|name| !matches!(name.as_str(), "admin" | "local" | "config"));
        names.sort();
        Ok(names)
    }

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
        let mut cache = self.columns.write().await;
        let columns = self.census_columns(table).await?;
        cache.insert(table.into(), columns.clone());
        Ok(columns)
    }

    async fn fetch_rows(
        &self,
        _schema: Option<&str>,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<QueryResult, DriverError> {
        let mut columns = self.page_columns(table).await?;
        let coll = self.db().collection::<Document>(table);
        let mut docs = Vec::new();
        if limit > 0 {
            let mut cursor = coll
                .find(doc! {})
                .sort(doc! { "_id": 1 })
                .skip(offset)
                .limit(limit.min(i64::MAX as u64) as i64)
                .await
                .map_err(map_mongo_error)?;
            while let Some(doc) = cursor.try_next().await.map_err(map_mongo_error)? {
                docs.push(doc);
            }
        }
        let mut cache = self.columns.write().await;
        if let Some(cached) = cache.get(table) {
            columns = cached.clone();
        }
        merge_page_types(&mut columns, &docs);
        cache.insert(table.into(), columns.clone());
        drop(cache);
        let rows = docs.iter().map(|doc| document_to_row(doc, &columns)).collect();
        Ok(QueryResult {
            columns,
            rows,
            truncated: false,
        })
    }

    async fn query_params(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DriverError> {
        if params.is_empty() {
            return self.query(sql).await;
        }
        let Some(query) = parse_keyed_value_select(sql, params, &self.database_name)? else {
            return Err(DriverError::Unsupported(
                "MongoDB bound reads support a single selected value by _id".into(),
            ));
        };
        self.run_keyed_value_select(query).await
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
                    position: None,
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
                    position: None,
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
            self.invalidate_columns(&coll).await;
            return Ok(ExecResult { rows_affected: 1 });
        }
        if let Some((coll, filter)) = parse_delete_many(trimmed) {
            let result = self
                .db()
                .collection::<Document>(&coll)
                .delete_many(filter)
                .await
                .map_err(map_mongo_error)?;
            self.invalidate_columns(&coll).await;
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
            self.invalidate_columns(&coll).await;
            return Ok(ExecResult { rows_affected: 0 });
        }
        Err(DriverError::Unsupported(
            "MongoDB execute supports insertOne/deleteMany shell forms and DROP TABLE only in this MVP".into(),
        ))
    }

    async fn execute_params(&self, sql: &str, params: &[Value]) -> Result<ExecResult, DriverError> {
        if let Some(update) = parse_keyed_update(sql, params, &self.database_name)? {
            let result = self
                .db()
                .collection::<Document>(&update.collection)
                .update_one(update.filter, doc! { "$set": update.set })
                .await
                .map_err(map_mongo_error)?;
            self.invalidate_columns(&update.collection).await;
            return Ok(ExecResult {
                rows_affected: result.matched_count,
            });
        }
        if let Some(delete) = parse_keyed_delete(sql, params, &self.database_name)? {
            let result = self
                .db()
                .collection::<Document>(&delete.collection)
                .delete_one(delete.filter)
                .await
                .map_err(map_mongo_error)?;
            self.invalidate_columns(&delete.collection).await;
            return Ok(ExecResult {
                rows_affected: result.deleted_count,
            });
        }
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

impl MongodbConnection {
    async fn run_keyed_value_select(&self, query: update::KeyedValueSelect) -> Result<QueryResult, DriverError> {
        let mut cursor = self
            .db()
            .collection::<Document>(&query.collection)
            .find(doc! { "_id": query.key })
            .projection(doc! { &query.column: 1 })
            .limit(2)
            .await
            .map_err(map_mongo_error)?;
        let mut docs = Vec::new();
        while docs.len() < 2 {
            let Some(document) = cursor.try_next().await.map_err(map_mongo_error)? else {
                break;
            };
            docs.push(document);
        }
        let data_type = docs
            .first()
            .and_then(|document| document.get(&query.column))
            .map(codec::bson_type_name)
            .unwrap_or_else(|| "unknown".into());
        let columns = vec![ColumnInfo {
            name: query.column.clone(),
            data_type,
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }];
        let rows = docs
            .iter()
            .map(|document| {
                let value = document.get(&query.column).ok_or_else(|| DriverError::Query {
                    message: "MongoDB value field is missing".into(),
                    sqlstate: None,
                    position: None,
                })?;
                Ok(vec![bson_to_value(value)])
            })
            .collect::<Result<Vec<_>, DriverError>>()?;
        Ok(QueryResult {
            columns,
            rows,
            truncated: docs.len() > 1,
        })
    }

    async fn run_find(&self, q: FindQuery) -> Result<QueryResult, DriverError> {
        let mut columns = self.page_columns(&q.collection).await?;
        let coll = self.db().collection::<Document>(&q.collection);
        let mut cursor = coll
            .find(q.filter)
            .sort(doc! { "_id": 1 })
            .skip(q.skip)
            .limit(q.limit)
            .await
            .map_err(map_mongo_error)?;
        let mut docs = Vec::new();
        let mut source_bytes = 0usize;
        let mut source_cells = 0usize;
        let mut truncated = false;
        while let Some(doc) = cursor.try_next().await.map_err(map_mongo_error)? {
            let size = mongodb::bson::to_vec(&doc)
                .map_err(|_| DriverError::Internal("failed to size BSON result".into()))?
                .len();
            let cells = source_cells.saturating_add(doc.len());
            if docs.len() >= MAX_QUERY_ROWS
                || source_bytes.saturating_add(size) > MAX_QUERY_RESULT_BYTES
                || cells > MAX_QUERY_RESULT_CELLS
            {
                truncated = true;
                break;
            }
            source_bytes += size;
            source_cells = cells;
            docs.push(doc);
        }
        merge_page_types(&mut columns, &docs);
        let (rows, row_truncated) = bounded_document_rows(&docs, &columns);
        Ok(QueryResult {
            columns,
            rows,
            truncated: truncated || row_truncated,
        })
    }

    async fn run_aggregate(&self, q: AggregateQuery) -> Result<QueryResult, DriverError> {
        let coll = self.db().collection::<Document>(&q.collection);
        let mut cursor = coll.aggregate(q.pipeline).await.map_err(map_mongo_error)?;
        let mut docs = Vec::new();
        let mut source_bytes = 0usize;
        let mut source_cells = 0usize;
        let mut truncated = false;
        while let Some(doc) = cursor.try_next().await.map_err(map_mongo_error)? {
            let size = mongodb::bson::to_vec(&doc)
                .map_err(|_| DriverError::Internal("failed to size BSON result".into()))?
                .len();
            let cells = source_cells.saturating_add(doc.len());
            if docs.len() >= MAX_QUERY_ROWS
                || source_bytes.saturating_add(size) > MAX_QUERY_RESULT_BYTES
                || cells > MAX_QUERY_RESULT_CELLS
            {
                truncated = true;
                break;
            }
            source_bytes += size;
            source_cells = cells;
            docs.push(doc);
        }
        let columns = columns_from_docs(&docs);
        let (rows, row_truncated) = bounded_document_rows(&docs, &columns);
        Ok(QueryResult {
            columns,
            rows,
            truncated: truncated || row_truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_session_is_refused() {
        let client_opts = ClientOptions::parse("mongodb://127.0.0.1:27017").await.unwrap();
        let client = Client::with_options(client_opts).unwrap();
        let conn = MongodbConnection::new(client, "test".into());

        match conn.open_session().await {
            Err(DriverError::Unsupported(_)) => {}
            Ok(_) => panic!("mongodb must refuse open_session"),
            Err(other) => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_timed_out_read_reports_an_unknown_outcome() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { while listener.accept().await.is_ok() {} });
        let opts = ConnectOptions {
            host: "127.0.0.1".into(),
            port,
            database: "test".into(),
            ..Default::default()
        };
        let client_opts = build_client_options(&opts).await.unwrap();
        let client = Client::with_options(client_opts).unwrap();
        let conn = MongodbConnection::new(client, "test".into());
        let control = tablepro_core::OperationControl::with_timeout(std::time::Duration::from_millis(50));

        let result = conn.list_tables_controlled(&control).await;

        match result {
            Err(DriverError::OperationOutcomeUnknown { .. }) => {}
            other => panic!("expected OperationOutcomeUnknown, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn value_contract_mongodb_cancelled_inflight_read_is_unknown_not_disconnected() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let opts = ConnectOptions {
            host: "127.0.0.1".into(),
            port,
            database: "test".into(),
            ..Default::default()
        };
        let client_opts = build_client_options(&opts).await.unwrap();
        let client = Client::with_options(client_opts).unwrap();
        let conn = MongodbConnection::new(client, "test".into());
        let token = tokio_util::sync::CancellationToken::new();
        let control = tablepro_core::OperationControl::new(token.clone(), None);
        let operation = tokio::spawn(async move { conn.list_tables_controlled(&control).await });

        // Hold the accepted socket open so this is an in-flight operation,
        // rather than a connect failure or a server disconnect.
        let (_stream, _) = tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
            .await
            .expect("MongoDB did not connect to the localhost fixture")
            .unwrap();
        token.cancel();

        match operation.await.unwrap() {
            Err(DriverError::OperationOutcomeUnknown { source }) => {
                assert!(matches!(*source, DriverError::Cancelled), "{source:?}");
            }
            other => panic!("expected unknown cancelled outcome, not disconnection: {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "requires docker"]
    async fn browse_pages_reuse_the_census_until_refresh() {
        use mongodb::bson::{Document, doc};
        use std::sync::{Arc, Mutex};
        use testcontainers::{ImageExt, runners::AsyncRunner};
        use testcontainers_modules::mongo::Mongo;

        let container = Mongo::default().with_tag("7").start().await.unwrap();
        let host = container.get_host().await.unwrap().to_string();
        let port = container.get_host_port_ipv4(27017).await.unwrap();
        let mut options = ClientOptions::parse(format!("mongodb://{host}:{port}/appdb"))
            .await
            .unwrap();
        let finds = Arc::new(Mutex::new(Vec::<Document>::new()));
        options.command_event_handler = Some(mongodb::event::EventHandler::callback({
            let finds = finds.clone();
            move |event: mongodb::event::command::CommandEvent| {
                if let mongodb::event::command::CommandEvent::Started(event) = event
                    && event.command_name == "find"
                    && event.command.get_str("find").ok() == Some("browse_pages")
                {
                    finds.lock().unwrap().push(event.command);
                }
            }
        }));
        let client = Client::with_options(options).unwrap();
        let database = client.database("appdb");
        let coll = database.collection::<Document>("browse_pages");
        coll.insert_many([doc! { "value": 1 }, doc! { "value": 2 }])
            .await
            .unwrap();
        let connection = MongodbConnection::new(client, "appdb".into());

        connection.fetch_columns(None, "browse_pages").await.unwrap();
        assert_eq!(finds.lock().unwrap().len(), 1);
        connection.fetch_rows(None, "browse_pages", 0, 1).await.unwrap();
        connection.fetch_rows(None, "browse_pages", 1, 1).await.unwrap();
        {
            let commands = finds.lock().unwrap();
            assert_eq!(commands.len(), 3);
            assert_eq!(commands[0].get("limit"), None);
            assert_eq!(commands[1].get_i64("limit").unwrap(), 1);
            assert_eq!(commands[1].get_document("sort").unwrap().get_i32("_id").unwrap(), 1);
            assert_eq!(commands[2].get_i64("limit").unwrap(), 1);
            assert_eq!(commands[2].get_i64("skip").unwrap(), 1);
            assert_eq!(commands[2].get_document("sort").unwrap().get_i32("_id").unwrap(), 1);
        }
        assert_empty_collection_and_zero_limit_contract(&connection, &database, &finds).await;

        connection.fetch_columns(None, "browse_pages").await.unwrap();
        assert_eq!(finds.lock().unwrap().len(), 4);
        connection
            .execute(r#"db.browse_pages.insertOne({"fresh": true})"#)
            .await
            .unwrap();
        connection.fetch_rows(None, "browse_pages", 0, 1).await.unwrap();
        {
            let commands = finds.lock().unwrap();
            assert_eq!(commands.len(), 6);
            assert_eq!(commands[4].get("limit"), None);
            assert_eq!(commands[5].get_i64("limit").unwrap(), 1);
        }
        assert_paged_find_queries_reuse_census(&connection, &finds).await;
    }

    #[tokio::test]
    #[ignore = "requires docker"]
    async fn concurrent_first_pages_share_one_census() {
        use mongodb::bson::{Document, doc};
        use std::sync::{Arc, Mutex};
        use testcontainers::{ImageExt, runners::AsyncRunner};
        use testcontainers_modules::mongo::Mongo;

        let container = Mongo::default().with_tag("7").start().await.unwrap();
        let host = container.get_host().await.unwrap().to_string();
        let port = container.get_host_port_ipv4(27017).await.unwrap();
        let mut options = ClientOptions::parse(format!("mongodb://{host}:{port}/appdb"))
            .await
            .unwrap();
        let finds = Arc::new(Mutex::new(Vec::<Document>::new()));
        options.command_event_handler = Some(mongodb::event::EventHandler::callback({
            let finds = finds.clone();
            move |event: mongodb::event::command::CommandEvent| {
                if let mongodb::event::command::CommandEvent::Started(event) = event
                    && event.command_name == "find"
                    && event.command.get_str("find").ok() == Some("concurrent_cold_pages")
                {
                    finds.lock().unwrap().push(event.command);
                }
            }
        }));
        let client = Client::with_options(options).unwrap();
        let coll = client.database("appdb").collection::<Document>("concurrent_cold_pages");
        coll.insert_many([doc! { "value": 1 }, doc! { "value": 2 }])
            .await
            .unwrap();
        let connection = Arc::new(MongodbConnection::new(client, "appdb".into()));

        let (first, second) = tokio::join!(
            connection.fetch_rows(None, "concurrent_cold_pages", 0, 1),
            connection.fetch_rows(None, "concurrent_cold_pages", 1, 1),
        );
        assert_eq!(first.unwrap().rows.len(), 1);
        assert_eq!(second.unwrap().rows.len(), 1);
        {
            let commands = finds.lock().unwrap();
            assert_eq!(commands.len(), 3);
            assert_eq!(
                commands.iter().filter(|command| command.get("limit").is_none()).count(),
                1
            );
            assert!(
                commands
                    .iter()
                    .filter(|command| command.get("limit").is_some())
                    .all(|command| { command.get_i64("limit").unwrap() == 1 })
            );
        }
        assert_concurrent_page_types_are_merged(&connection, &coll).await;
    }

    async fn assert_concurrent_page_types_are_merged(
        connection: &MongodbConnection,
        coll: &mongodb::Collection<Document>,
    ) {
        coll.update_one(doc! { "value": 1 }, doc! { "$set": { "left_only": true } })
            .await
            .unwrap();
        coll.update_one(doc! { "value": 2 }, doc! { "$set": { "right_only": true } })
            .await
            .unwrap();
        let (first, second) = tokio::join!(
            connection.fetch_rows(None, "concurrent_cold_pages", 0, 1),
            connection.fetch_rows(None, "concurrent_cold_pages", 1, 1),
        );
        first.unwrap();
        second.unwrap();
        let columns = connection.columns.read().await;
        let names: Vec<_> = columns["concurrent_cold_pages"]
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        assert!(names.contains(&"left_only"));
        assert!(names.contains(&"right_only"));
    }

    async fn assert_empty_collection_and_zero_limit_contract(
        connection: &MongodbConnection,
        database: &mongodb::Database,
        finds: &std::sync::Arc<std::sync::Mutex<Vec<mongodb::bson::Document>>>,
    ) {
        database.create_collection("empty_columns").await.unwrap();
        let columns = connection.fetch_columns(None, "empty_columns").await.unwrap();
        assert!(
            columns
                .iter()
                .any(|column| column.name == "_id" && column.data_type == "ObjectId")
        );
        let page = connection.fetch_rows(None, "browse_pages", 0, 0).await.unwrap();
        assert!(page.rows.is_empty());
        let commands = finds.lock().unwrap();
        assert_eq!(commands.len(), 3);
    }

    async fn assert_paged_find_queries_reuse_census(
        connection: &MongodbConnection,
        finds: &std::sync::Arc<std::sync::Mutex<Vec<mongodb::bson::Document>>>,
    ) {
        connection
            .query("db.browse_pages.find({}).skip(0).limit(1)")
            .await
            .unwrap();
        connection
            .query("db.browse_pages.find({}).skip(1).limit(1)")
            .await
            .unwrap();
        let commands = finds.lock().unwrap();
        assert_eq!(commands.len(), 8);
        assert_eq!(commands[6].get_i64("limit").unwrap(), 1);
        assert_eq!(commands[6].get_document("sort").unwrap().get_i32("_id").unwrap(), 1);
        assert_eq!(commands[7].get_i64("limit").unwrap(), 1);
        assert_eq!(commands[7].get_i64("skip").unwrap(), 1);
        assert_eq!(commands[7].get_document("sort").unwrap().get_i32("_id").unwrap(), 1);
    }

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
        assert_eq!(d.default_database(), "test");
        assert_eq!(d.default_username(), "");
    }

    #[test]
    fn decimal128_edit_parser_preserves_native_precision_and_refuses_rounding() {
        for value in [
            "1234567890123456789012345678901234",
            "0.1234567890123456789012345678901234",
            "1.2300",
            "NaN",
            "Infinity",
            "-Infinity",
        ] {
            assert_eq!(canonical_decimal128_text(value).unwrap(), value, "{value}");
        }
        assert!(canonical_decimal128_text("12345678901234567890123456789012345").is_err());
        assert!(canonical_decimal128_text("1.2.3").is_err());
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
    fn mongo_host_authority_accepts_dns_and_ip_literals() {
        for (input, expected) in [
            ("db.internal", "db.internal"),
            ("db.internal.", "db.internal."),
            ("127.0.0.1", "127.0.0.1"),
            ("::1", "[::1]"),
            ("[::1]", "[::1]"),
        ] {
            assert_eq!(mongo_host_authority(input).unwrap(), expected);
        }
    }

    #[test]
    fn mongo_host_authority_rejects_uri_syntax_and_malformed_names() {
        for input in [
            "",
            "db/path",
            "db?replicaSet=evil",
            "db#fragment",
            "user@db",
            "db,other",
            "db:27018",
            "db%2fother",
            "db\\other",
            " db",
            "db ",
            "-db",
            "db..internal",
            "[::1",
            "::1]",
            "[127.0.0.1]",
        ] {
            assert!(mongo_host_authority(input).is_err(), "accepted {input:?}");
        }
    }

    #[tokio::test]
    async fn a_tunneled_connection_verifies_tls_against_the_service_hostname() {
        let opts = ConnectOptions {
            host: "127.0.0.1".into(),
            port: 54321,
            service_endpoint: Some(("mongo.internal.example".into(), 27017)),
            tls: tablepro_core::TlsConfig {
                mode: tablepro_core::TlsMode::VerifyFull,
                ..Default::default()
            },
            ..Default::default()
        };
        let client_opts = build_client_options(&opts).await.expect("build client options");
        let Some(Tls::Enabled(tls_options)) = client_opts.tls else {
            panic!("TLS must be enabled when verifying");
        };
        assert_eq!(tls_options.verify_hostname.as_deref(), Some("mongo.internal.example"));
    }
}
