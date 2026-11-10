#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{ConnectOptions, DatabaseDriver, DriverError, TlsConfig, Value};
use drivers_cassandra::CassandraDriver;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

async fn start_cassandra() -> (testcontainers::ContainerAsync<GenericImage>, ConnectOptions) {
    let image = GenericImage::new("cassandra", "5.0")
        .with_exposed_port(9042.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Starting listening for CQL clients"))
        .with_env_var("MAX_HEAP_SIZE", "512M")
        .with_env_var("HEAP_NEWSIZE", "128M");
    started(image).await
}

async fn start_scylla() -> (testcontainers::ContainerAsync<GenericImage>, ConnectOptions) {
    let image = GenericImage::new("scylladb/scylla", "2025.1")
        .with_exposed_port(9042.tcp())
        .with_wait_for(WaitFor::message_on_stderr("init - serving"))
        .with_cmd([
            "--smp",
            "1",
            "--memory",
            "512M",
            "--overprovisioned",
            "1",
            "--developer-mode",
            "1",
        ]);
    started(image).await
}

async fn started(
    image: testcontainers::core::ContainerRequest<GenericImage>,
) -> (testcontainers::ContainerAsync<GenericImage>, ConnectOptions) {
    let container = image
        .with_startup_timeout(std::time::Duration::from_secs(240))
        .start()
        .await
        .expect("start container");
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(9042).await.unwrap(),
        database: String::new(),
        username: "cassandra".into(),
        password: secrecy::SecretString::new("cassandra".to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    (container, options)
}

async fn seed(container: &testcontainers::ContainerAsync<GenericImage>) {
    let script = "CREATE KEYSPACE conf WITH replication = {'class': 'SimpleStrategy', 'replication_factor': 1}; \
         CREATE TABLE conf.items (id bigint PRIMARY KEY, label text, n bigint, price decimal, tags set<text>, \
         born date, at timestamp, uid uuid); \
         INSERT INTO conf.items (id, label, n, price, tags, born, at, uid) VALUES \
         (1, 'one', 9007199254740993, 123.45, {'a','b'}, '2026-11-10', '2026-11-10T12:00:00Z', \
          123e4567-e89b-12d3-a456-426614174000); \
         INSERT INTO conf.items (id, label) VALUES (2, ''); \
         INSERT INTO conf.items (id, n) VALUES (3, -5);";
    for _ in 0..60 {
        let mut result = container
            .exec(testcontainers::core::ExecCommand::new(["cqlsh", "-e", script]))
            .await
            .expect("exec cqlsh");
        let _ = result.stderr_to_vec().await;
        let _ = result.stdout_to_vec().await;
        if result.exit_code().await.expect("exit code") == Some(0) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    panic!("cqlsh never accepted the seed script");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_cassandra_keyspace_reads_back_typed_values_and_refuses_writes() {
    let (container, options) = start_cassandra().await;
    exercise(&container, options).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_scylladb_keyspace_reads_back_the_same_values_and_refuses_writes() {
    let (container, options) = start_scylla().await;
    exercise(&container, options).await;
}

async fn exercise(container: &testcontainers::ContainerAsync<GenericImage>, options: ConnectOptions) {
    seed(container).await;
    let connection = CassandraDriver.connect(options).await.expect("connect");
    connection.ping().await.expect("ping");

    let tables = connection.list_tables().await.unwrap();
    assert!(
        tables
            .iter()
            .any(|t| t.schema.as_deref() == Some("conf") && t.name == "items"),
        "{tables:?}"
    );
    assert!(
        tables
            .iter()
            .all(|t| !t.schema.as_deref().unwrap_or("").starts_with("system")),
        "{tables:?}"
    );

    let columns = connection.fetch_columns(Some("conf"), "items").await.unwrap();
    assert_eq!(columns[0].name, "id");
    assert!(columns[0].primary_key && !columns[0].nullable);
    assert!(columns[1..].iter().all(|column| !column.primary_key && column.nullable));
    let names: Vec<&str> = columns.iter().map(|column| column.name.as_str()).collect();
    assert_eq!(names, ["id", "at", "born", "label", "n", "price", "tags", "uid"]);

    let page = connection.fetch_rows(Some("conf"), "items", 0, 10).await.unwrap();
    let mut rows: Vec<Vec<Value>> = page.rows.iter().map(|row| row.to_vec()).collect();
    rows.sort_by_key(|row| match row[0] {
        Value::Int(id) => id,
        _ => i64::MAX,
    });
    let index = |name: &str| page.columns.iter().position(|column| column.name == name).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][index("n")], Value::Int(9_007_199_254_740_993));
    assert_eq!(rows[0][index("price")], Value::Decimal("123.45".parse().unwrap()));
    assert_eq!(rows[0][index("tags")], Value::JsonText(r#"["a","b"]"#.into()));
    assert_eq!(
        rows[0][index("born")].clone(),
        Value::Date("2026-11-10".parse().unwrap())
    );
    assert_eq!(
        rows[0][index("uid")].clone(),
        Value::Uuid("123e4567-e89b-12d3-a456-426614174000".parse().unwrap())
    );
    assert_eq!(
        rows[1][index("label")],
        Value::Text(String::new()),
        "an empty string is not NULL"
    );
    assert_eq!(rows[1][index("n")], Value::Null);
    assert_eq!(rows[2][index("n")], Value::Int(-5));

    let second = connection.fetch_rows(Some("conf"), "items", 2, 2).await.unwrap();
    assert_eq!(second.rows.iter().count(), 1, "offset 2 of 3 rows leaves one");

    let selected = connection
        .query("SELECT label FROM conf.items WHERE id = 1;")
        .await
        .unwrap();
    assert_eq!(selected.rows.iter().next().unwrap()[0], Value::Text("one".into()));

    for cql in [
        "INSERT INTO conf.items (id) VALUES (9)",
        "DELETE FROM conf.items WHERE id = 1",
        "DROP TABLE conf.items",
    ] {
        assert!(
            matches!(connection.query(cql).await, Err(DriverError::ReadOnly)),
            "{cql}"
        );
        assert!(
            matches!(connection.execute(cql).await, Err(DriverError::ReadOnly)),
            "{cql}"
        );
    }
    let count = connection.query("SELECT count(*) FROM conf.items").await.unwrap();
    assert_eq!(
        count.rows.iter().next().unwrap()[0],
        Value::Int(3),
        "nothing was written"
    );
}
