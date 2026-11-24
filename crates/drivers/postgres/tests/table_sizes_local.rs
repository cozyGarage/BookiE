#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{ConnectOptions, DatabaseDriver};
use drivers_postgres::PgDriver;

fn opts_from_env() -> ConnectOptions {
    let port = std::env::var("SMOKE_PG_PORT").unwrap_or_else(|_| "54329".into());
    ConnectOptions {
        host: std::env::var("SMOKE_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: port
            .parse()
            .unwrap_or_else(|error| panic!("SMOKE_PG_PORT={port} is not a port number: {error}")),
        database: std::env::var("SMOKE_PG_DB").unwrap_or_else(|_| "bookie".into()),
        username: std::env::var("SMOKE_PG_USER").unwrap_or_else(|_| "bookie".into()),
        password: secrecy::SecretString::new(
            std::env::var("SMOKE_PG_PASS")
                .unwrap_or_else(|_| "bookie".into())
                .into(),
        ),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "requires a local postgres; set SMOKE_PG_*"]
async fn table_sizes_come_from_catalog_statistics_for_one_schema() {
    let connection = PgDriver.connect(opts_from_env()).await.unwrap();
    let run = std::process::id();
    let schema = format!("sizes_{run}");
    connection.execute(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    connection
        .execute(&format!(
            "CREATE TABLE {schema}.filled AS SELECT g AS id, repeat('x', 200) AS body FROM generate_series(1, 5000) g"
        ))
        .await
        .unwrap();
    connection
        .execute(&format!("CREATE INDEX ON {schema}.filled (id)"))
        .await
        .unwrap();
    connection
        .execute(&format!("CREATE TABLE {schema}.empty (id int)"))
        .await
        .unwrap();
    connection.execute(&format!("ANALYZE {schema}.filled")).await.unwrap();

    let sizes = connection.table_sizes(Some(&schema)).await.unwrap();
    let _ = connection.execute(&format!("DROP SCHEMA {schema} CASCADE")).await;

    assert_eq!(sizes.len(), 2, "{sizes:?}");
    let filled = sizes
        .iter()
        .find(|size| size.name == "filled")
        .expect("filled is listed");
    assert_eq!(filled.schema.as_deref(), Some(schema.as_str()));
    assert!(
        filled.bytes > 1_000_000,
        "5000 rows of 200 bytes plus an index: {filled:?}"
    );
    assert_eq!(
        sizes.iter().find(|size| size.name == "empty").map(|size| size.bytes),
        Some(0)
    );
    let everything = connection.table_sizes(None).await.unwrap();
    assert!(
        everything
            .iter()
            .all(|size| size.schema.as_deref() != Some("pg_catalog"))
    );
}
