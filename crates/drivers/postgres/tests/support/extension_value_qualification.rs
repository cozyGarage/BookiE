#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bookie_core::{ConnectOptions, DatabaseDriver, TlsConfig, Value};
use drivers_postgres::PgDriver;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

async fn start_extension_postgres(image: &str, tag: &str) -> (ContainerAsync<GenericImage>, ConnectOptions) {
    let image = GenericImage::new(image.to_owned(), tag.to_owned())
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stdout(
            "database system is ready to accept connections",
        ));
    let image = match std::env::var("BOOKIE_TEST_RUN_ID") {
        Ok(run_id) => image
            .with_env_var("POSTGRES_PASSWORD", "postgres")
            .with_label("com.bookie.test-run", run_id),
        Err(_) => image.with_env_var("POSTGRES_PASSWORD", "postgres"),
    };
    let container = image
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("start PostgreSQL extension fixture");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: secrecy::SecretString::new("postgres".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    (container, options)
}

#[tokio::test]
#[ignore = "requires docker"]
async fn pgvector_values_are_refused_without_losing_null_or_type_identity() {
    let (_container, options) = start_extension_postgres("pgvector/pgvector", "0.8.7-pg16").await;
    let connection = PgDriver.connect(options).await.expect("connect to pgvector");
    connection
        .execute("CREATE EXTENSION vector")
        .await
        .expect("enable pgvector");
    let version = connection
        .query("SELECT extversion FROM pg_extension WHERE extname = 'vector'")
        .await
        .expect("read pgvector version");
    assert_eq!(version.rows, vec![vec![Value::Text("0.8.7".into())]]);

    let result = connection
        .query("SELECT '[1,2,3]'::vector(3) AS value, NULL::vector(3) AS missing")
        .await
        .expect("read vector values");
    assert_eq!(result.columns[0].data_type, "vector");
    assert_eq!(
        result.rows,
        vec![vec![Value::Undecodable("VECTOR".into()), Value::Null]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn postgis_values_are_refused_without_losing_empty_null_or_spatial_metadata() {
    let (_container, options) = start_extension_postgres("postgis/postgis", "16-3.7").await;
    let connection = PgDriver.connect(options).await.expect("connect to PostGIS");
    connection
        .execute("CREATE EXTENSION IF NOT EXISTS postgis")
        .await
        .expect("enable PostGIS");
    let version = connection
        .query("SELECT extversion FROM pg_extension WHERE extname = 'postgis'")
        .await
        .expect("read PostGIS version");
    assert_eq!(version.rows, vec![vec![Value::Text("3.7.0".into())]]);

    let result = connection
        .query(
            "SELECT geom, ST_SRID(geom)::bigint, ST_NDims(geom), ST_IsEmpty(geom) \
             FROM (VALUES \
                 (ST_GeomFromEWKT('SRID=3857;POINT Z(1 2 3)')), \
                 (ST_GeomFromText('POINT EMPTY')), \
                 (NULL::geometry) \
             ) AS samples(geom)",
        )
        .await
        .expect("read geometry values");
    assert_eq!(result.columns[0].data_type, "geometry");
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Undecodable("GEOMETRY".into()),
                Value::Int(3857),
                Value::Int(3),
                Value::Bool(false)
            ],
            vec![
                Value::Undecodable("GEOMETRY".into()),
                Value::Int(0),
                Value::Int(2),
                Value::Bool(true)
            ],
            vec![Value::Null, Value::Null, Value::Null, Value::Null],
        ]
    );
}
