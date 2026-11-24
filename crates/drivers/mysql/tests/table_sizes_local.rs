#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{ConnectOptions, DatabaseDriver};
use drivers_mysql::MysqlDriver;

fn opts_from_env() -> ConnectOptions {
    ConnectOptions {
        host: std::env::var("SMOKE_MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        port: std::env::var("SMOKE_MYSQL_PORT")
            .unwrap_or_else(|_| "3306".into())
            .parse()
            .unwrap(),
        database: std::env::var("SMOKE_MYSQL_DB").unwrap_or_else(|_| "mysql".into()),
        username: std::env::var("SMOKE_MYSQL_USER").unwrap_or_else(|_| "root".into()),
        password: secrecy::SecretString::new(std::env::var("SMOKE_MYSQL_PASS").unwrap_or_default().into()),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "requires a running MySQL or MariaDB; set SMOKE_MYSQL_*"]
async fn table_sizes_list_base_tables_of_the_current_or_named_schema() {
    let options = opts_from_env();
    let schema = options.database.clone();
    let connection = MysqlDriver.connect(options).await.unwrap();
    let table = format!("bookie_sizes_{}", std::process::id());
    connection
        .execute(&format!("CREATE TABLE {table} (id INT PRIMARY KEY, body VARCHAR(200))"))
        .await
        .unwrap();
    connection
        .execute(&format!("CREATE VIEW {table}_view AS SELECT id FROM {table}"))
        .await
        .unwrap();

    let current = connection.table_sizes(None).await;
    let named = connection.table_sizes(Some(&schema)).await;
    let _ = connection.execute(&format!("DROP VIEW {table}_view")).await;
    let _ = connection.execute(&format!("DROP TABLE {table}")).await;

    for sizes in [current.unwrap(), named.unwrap()] {
        let row = sizes
            .iter()
            .find(|size| size.name == table)
            .expect("the new table is listed");
        assert_eq!(row.schema.as_deref(), Some(schema.as_str()));
        assert!(row.bytes > 0, "{row:?}");
        assert!(
            sizes.iter().all(|size| size.name != format!("{table}_view")),
            "views have no size row"
        );
    }
}
