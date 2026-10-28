#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use secrecy::SecretString;
use tablepro_app_services::browse_query::{BrowseTarget, PageQuery, mark_partial_cells, parse_partial};
use tablepro_core::{ColumnInfo, ConnectOptions, Connection, DatabaseDriver, FilterSet, TlsConfig, Value};
use testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

async fn assert_preview_page(driver_id: &str, connection: &dyn Connection, columns: &[ColumnInfo], table: &str) {
    let filter = FilterSet::default();
    let target = BrowseTarget {
        driver_id,
        schema: None,
        table,
        columns,
        filter: &filter,
        hidden_columns: None,
        long_text_preview: true,
    };
    let PageQuery::Sql(query) = target.page(0, 10, None, None).unwrap() else {
        panic!("{driver_id}: a preview page must run as SQL")
    };

    let mut result = connection.query_params(&query.sql, &query.params).await.unwrap();
    let wire: usize = result
        .rows
        .iter()
        .flatten()
        .map(|cell| if let Value::Text(text) = cell { text.len() } else { 0 })
        .sum();
    mark_partial_cells(&mut result);

    assert!(
        wire < 16 * 1024,
        "{driver_id}: only a small preview may cross the wire: {wire}"
    );
    let Value::Undecodable(partial) = &result.rows[0][1] else {
        panic!("{driver_id}: the huge cell is partial: {:?}", result.rows[0][1])
    };
    assert_eq!(
        parse_partial(partial),
        Some((2_097_152, "x".repeat(1024).as_str())),
        "{driver_id}"
    );
    assert_eq!(result.rows[1][1], Value::Text("short".into()), "{driver_id}");
    assert_eq!(result.rows[2][1], Value::Null, "{driver_id}");
}

#[tokio::test]
async fn sqlite_previews_a_huge_text_cell() {
    let directory = tempfile::TempDir::new().unwrap();
    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: directory.path().join("preview.db").to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO notes VALUES (1, printf('%.*c', 2097152, 'x')), (2, 'short'), (3, NULL)")
        .await
        .unwrap();
    let columns = [column("id", "INTEGER", true), column("body", "TEXT", false)];

    assert_preview_page("sqlite", connection.as_ref(), &columns, "notes").await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_previews_a_huge_longtext_cell() {
    let container = testcontainers_modules::mysql::Mysql::default()
        .with_tag("8.4")
        .with_env_var("MYSQL_ROOT_PASSWORD", "tablepro_test")
        .start()
        .await
        .unwrap();
    let connection = drivers_mysql::MysqlDriver
        .connect(ConnectOptions {
            host: container.get_host().await.unwrap().to_string(),
            port: container.get_host_port_ipv4(3306).await.unwrap(),
            database: "test".into(),
            username: "root".into(),
            password: SecretString::from("tablepro_test"),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE notes (id INT PRIMARY KEY, body LONGTEXT)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO notes VALUES (1, REPEAT('x', 2097152)), (2, 'short'), (3, NULL)")
        .await
        .unwrap();
    let columns = [column("id", "int", true), column("body", "longtext", false)];

    assert_preview_page("mysql", connection.as_ref(), &columns, "notes").await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn sql_server_previews_a_huge_varchar_max_cell() {
    let container = testcontainers_modules::mssql_server::MssqlServer::default()
        .with_accept_eula()
        .with_startup_timeout(std::time::Duration::from_secs(180))
        .start()
        .await
        .unwrap();
    let options = ConnectOptions {
        host: container.get_host().await.unwrap().to_string(),
        port: container.get_host_port_ipv4(1433).await.unwrap(),
        database: "master".into(),
        username: "sa".into(),
        password: SecretString::new(
            testcontainers_modules::mssql_server::MssqlServer::DEFAULT_SA_PASSWORD
                .to_string()
                .into(),
        ),
        tls: TlsConfig::disabled(),
        ..Default::default()
    };
    let mut connection = None;
    for _ in 0..60 {
        if let Ok(opened) = drivers_mssql::MssqlDriver.connect(options.clone()).await
            && opened.query("SELECT 1").await.is_ok()
        {
            connection = Some(opened);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    let connection = connection.expect("SQL Server accepts a login");
    connection
        .execute("CREATE TABLE notes (id INT PRIMARY KEY, body VARCHAR(MAX))")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO notes VALUES (1, REPLICATE(CAST('x' AS VARCHAR(MAX)), 2097152)), (2, 'short'), (3, NULL)")
        .await
        .unwrap();
    let columns = [column("id", "int", true), column("body", "varchar(max)", false)];

    assert_preview_page("mssql", connection.as_ref(), &columns, "notes").await;
}
