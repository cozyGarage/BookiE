#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bookie_core::{ConnectOptions, Connection, DatabaseDriver, TlsConfig, Value};
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

async fn assert_pgvector_type_identity(connection: &dyn Connection) {
    connection
        .execute("CREATE SCHEMA extension_types")
        .await
        .expect("create extension schema");
    connection
        .execute("CREATE EXTENSION vector WITH SCHEMA extension_types")
        .await
        .expect("enable pgvector outside public");
    connection
        .execute("CREATE TYPE public.vector AS (label text)")
        .await
        .expect("create same-named user type");

    let identity = connection
        .query(
            "SELECT extension_type.oid::bigint, user_type.oid::bigint, extension_namespace.nspname, \
             EXISTS (SELECT 1 FROM pg_depend dependency \
                     JOIN pg_extension extension ON extension.oid = dependency.refobjid \
                     WHERE dependency.classid = 'pg_type'::regclass \
                       AND dependency.objid = extension_type.oid \
                       AND dependency.refclassid = 'pg_extension'::regclass \
                       AND dependency.deptype = 'e' AND extension.extname = 'vector') \
             FROM pg_type extension_type \
             JOIN pg_namespace extension_namespace ON extension_namespace.oid = extension_type.typnamespace \
             CROSS JOIN pg_type user_type \
             JOIN pg_namespace user_namespace ON user_namespace.oid = user_type.typnamespace \
             WHERE extension_type.typname = 'vector' AND extension_namespace.nspname = 'extension_types' \
               AND user_type.typname = 'vector' AND user_namespace.nspname = 'public'",
        )
        .await
        .expect("read vector type identity");
    assert_eq!(identity.rows.len(), 1);
    assert_ne!(identity.rows[0][0], identity.rows[0][1]);
    assert_eq!(identity.rows[0][2], Value::Text("extension_types".into()));
    assert_eq!(identity.rows[0][3], Value::Bool(true));
}

async fn assert_postgis_type_identity(connection: &dyn Connection) {
    connection
        .execute("CREATE SCHEMA lookalike")
        .await
        .expect("create lookalike schema");
    connection
        .execute("CREATE TYPE lookalike.geometry AS (label text)")
        .await
        .expect("create same-named user type");
    connection
        .execute("CREATE EXTENSION IF NOT EXISTS postgis")
        .await
        .expect("enable PostGIS");

    let identity = connection
        .query(
            "SELECT extension_type.oid::bigint, user_type.oid::bigint, extension_namespace.nspname, \
             EXISTS (SELECT 1 FROM pg_depend dependency \
                     JOIN pg_extension extension ON extension.oid = dependency.refobjid \
                     WHERE dependency.classid = 'pg_type'::regclass \
                       AND dependency.objid = extension_type.oid \
                       AND dependency.refclassid = 'pg_extension'::regclass \
                       AND dependency.deptype = 'e' AND extension.extname = 'postgis') \
             FROM pg_type extension_type \
             JOIN pg_namespace extension_namespace ON extension_namespace.oid = extension_type.typnamespace \
             CROSS JOIN pg_type user_type \
             JOIN pg_namespace user_namespace ON user_namespace.oid = user_type.typnamespace \
             WHERE extension_type.typname = 'geometry' AND extension_namespace.nspname = 'public' \
               AND user_type.typname = 'geometry' AND user_namespace.nspname = 'lookalike'",
        )
        .await
        .expect("read geometry type identity");
    assert_eq!(identity.rows.len(), 1);
    assert_ne!(identity.rows[0][0], identity.rows[0][1]);
    assert_eq!(identity.rows[0][2], Value::Text("public".into()));
    assert_eq!(identity.rows[0][3], Value::Bool(true));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn pgvector_values_are_refused_without_losing_null_or_type_identity() {
    let (_container, options) = start_extension_postgres("pgvector/pgvector", "0.8.7-pg16").await;
    let connection = PgDriver.connect(options).await.expect("connect to pgvector");
    assert_pgvector_type_identity(connection.as_ref()).await;
    let version = connection
        .query("SELECT extversion FROM pg_extension WHERE extname = 'vector'")
        .await
        .expect("read pgvector version");
    assert_eq!(version.rows, vec![vec![Value::Text("0.8.7".into())]]);

    let result = connection
        .query(
            "SELECT value, extension_types.vector_dims(value), extension_types.vector_send(value), \
             value::text, NULL::extension_types.vector(3), ROW('user')::public.vector \
             FROM (SELECT '[1,0.1,3.1415927]'::extension_types.vector(3) AS value) sample",
        )
        .await
        .expect("read vector values");
    assert_eq!(result.columns[0].data_type, "extension_types.vector");
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Undecodable("EXTENSION_TYPES.VECTOR".into()),
            Value::Int(3),
            Value::Bytes(vec![
                0, 3, 0, 0, 0x3f, 0x80, 0, 0, 0x3d, 0xcc, 0xcc, 0xcd, 0x40, 0x49, 0x0f, 0xdb,
            ]),
            Value::Text("[1,0.1,3.1415927]".into()),
            Value::Null,
            Value::Undecodable("VECTOR".into()),
        ]]
    );
    assert_eq!(result.columns[5].data_type, "vector");

    let invalid_dimensions = connection.query("SELECT '[1,2]'::extension_types.vector(3)").await;
    assert!(invalid_dimensions.is_err());
    assert_eq!(
        connection
            .query("SELECT 1")
            .await
            .expect("connection remains usable")
            .rows,
        vec![vec![Value::Int(1)]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn postgis_values_are_refused_without_losing_empty_null_or_spatial_metadata() {
    let (_container, options) = start_extension_postgres("postgis/postgis", "16-3.7").await;
    let connection = PgDriver.connect(options).await.expect("connect to PostGIS");
    assert_postgis_type_identity(connection.as_ref()).await;
    let version = connection
        .query("SELECT extversion FROM pg_extension WHERE extname = 'postgis'")
        .await
        .expect("read PostGIS version");
    assert_eq!(version.rows, vec![vec![Value::Text("3.7.0".into())]]);

    let result = connection
        .query(
            "SELECT geom, ST_AsEWKB(geom, 'XDR'), ST_SRID(geom)::bigint, ST_NDims(geom), ST_IsEmpty(geom), \
             ROW('user')::lookalike.geometry \
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
                Value::Bytes(vec![
                    0, 0xa0, 0, 0, 1, 0, 0, 0x0f, 0x11, 0x3f, 0xf0, 0, 0, 0, 0, 0, 0, 0x40, 0, 0, 0, 0, 0, 0, 0, 0x40,
                    8, 0, 0, 0, 0, 0, 0,
                ]),
                Value::Int(3857),
                Value::Int(3),
                Value::Bool(false),
                Value::Undecodable("LOOKALIKE.GEOMETRY".into())
            ],
            vec![
                Value::Undecodable("GEOMETRY".into()),
                Value::Bytes(vec![
                    0, 0, 0, 0, 1, 0x7f, 0xf8, 0, 0, 0, 0, 0, 0, 0x7f, 0xf8, 0, 0, 0, 0, 0, 0,
                ]),
                Value::Int(0),
                Value::Int(2),
                Value::Bool(true),
                Value::Undecodable("LOOKALIKE.GEOMETRY".into())
            ],
            vec![
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Undecodable("LOOKALIKE.GEOMETRY".into())
            ],
        ]
    );
    assert_eq!(result.columns[5].data_type, "lookalike.geometry");
}
