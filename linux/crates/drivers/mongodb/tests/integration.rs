#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_mongodb::MongodbDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver, TlsConfig, Value};
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::mongo::Mongo;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// The module's own default tag is MongoDB 5.0.6, which is long out of
/// support and is not the server this driver is verified against. Pin the
/// same major the TLS fixture uses so both tiers exercise one version.
const MONGO_TAG: &str = "7";

async fn start_mongo() -> (ContainerAsync<Mongo>, String, u16) {
    let container = Mongo::default()
        .with_tag(MONGO_TAG)
        .start()
        .await
        .expect("start mongo container");
    let host = container.get_host().await.expect("host").to_string();
    let port = container.get_host_port_ipv4(27017).await.expect("port");
    (container, host, port)
}

fn opts(host: &str, port: u16, database: &str) -> ConnectOptions {
    ConnectOptions {
        host: host.to_string(),
        port,
        database: database.to_string(),
        username: String::new(),
        password: secrecy::SecretString::new(String::new().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    }
}

async fn seeded_connection(host: &str, port: u16) -> Box<dyn tablepro_core::Connection> {
    let conn = MongodbDriver.connect(opts(host, port, "appdb")).await.expect("connect");
    conn.execute(r#"db.people.insertOne({"name": "ada", "team": "core"})"#)
        .await
        .expect("seed ada");
    conn.execute(r#"db.people.insertOne({"name": "grace", "team": "core"})"#)
        .await
        .expect("seed grace");
    conn.execute(r#"db.people.insertOne({"name": "alan", "team": "ops"})"#)
        .await
        .expect("seed alan");
    conn
}

fn column_values(result: &tablepro_core::QueryResult, column: &str) -> Vec<String> {
    let index = result
        .columns
        .iter()
        .position(|c| c.name == column)
        .unwrap_or_else(|| panic!("column {column} missing from {:?}", result.columns));
    result
        .rows
        .iter()
        .filter_map(|row| match row.get(index) {
            Some(Value::Text(text)) => Some(text.clone()),
            Some(Value::Json(json)) => Some(json.to_string().trim_matches('"').to_string()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_inserted_document_is_listed_browsed_and_found() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let collections = conn.list_tables().await.expect("list collections");
    assert!(
        collections.iter().any(|c| c.name == "people"),
        "the seeded collection must be listed: {collections:?}"
    );

    let browsed = conn.fetch_rows(None, "people", 0, 10).await.expect("browse people");
    assert_eq!(browsed.rows.len(), 3, "browse must return every seeded document");

    let found = conn
        .query(r#"db.people.find({"team": "core"})"#)
        .await
        .expect("find by filter");
    let mut names = column_values(&found, "name");
    names.sort();
    assert_eq!(names, vec!["ada".to_string(), "grace".to_string()]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_filtered_delete_removes_only_the_matching_documents() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let deleted = conn
        .execute(r#"db.people.deleteMany({"team": "ops"})"#)
        .await
        .expect("delete ops");
    assert_eq!(deleted.rows_affected, 1, "only the ops document matches");

    let remaining = conn.fetch_rows(None, "people", 0, 10).await.expect("browse people");
    assert_eq!(remaining.rows.len(), 2, "the core documents must survive");
    let mut names = column_values(&remaining, "name");
    names.sort();
    assert_eq!(names, vec!["ada".to_string(), "grace".to_string()]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_aggregate_pipeline_groups_documents() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let result = conn
        .query(r#"db.people.aggregate([{"$group": {"_id": "$team", "total": {"$sum": 1}}}])"#)
        .await
        .expect("aggregate by team");

    assert_eq!(result.rows.len(), 2, "core and ops must each produce a group");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_dropped_collection_stops_being_listed() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    conn.execute("DROP TABLE people").await.expect("drop people");

    let collections = conn.list_tables().await.expect("list collections");
    assert!(
        !collections.iter().any(|c| c.name == "people"),
        "the dropped collection must disappear: {collections:?}"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_unsupported_statement_is_refused_rather_than_silently_ignored() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let error = conn
        .execute("UPDATE people SET name = 'x'")
        .await
        .expect_err("the driver documents insertOne/deleteMany/DROP TABLE only");
    let message = format!("{error}");
    assert!(
        message.contains("insertOne") || message.contains("unsupported"),
        "the refusal must say what is supported, got: {message}"
    );

    let survivors = conn.fetch_rows(None, "people", 0, 10).await.expect("browse people");
    assert_eq!(survivors.rows.len(), 3, "a refused statement must change nothing");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn browsing_a_missing_collection_is_empty_rather_than_an_error() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let browsed = conn
        .fetch_rows(None, "absent", 0, 10)
        .await
        .expect("browsing an absent collection must not fail");
    assert!(browsed.rows.is_empty(), "an absent collection has no rows");
}
