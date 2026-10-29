#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_sqlite::SqliteDriver;
use tablepro_core::sql_literal::render_sql_literal;
use tablepro_core::{ConnectOptions, DatabaseDriver, Value};
use tempfile::TempDir;

#[tokio::test]
async fn a_text_value_with_a_nul_byte_survives_a_generated_sqlite_literal() {
    let directory = TempDir::new().unwrap();
    let connection = SqliteDriver
        .connect(ConnectOptions {
            database: directory.path().join("nul.db").to_string_lossy().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let literal = render_sql_literal("sqlite", &Value::Text("a\0b".into())).unwrap();

    let result = connection
        .query(&format!("SELECT {literal}, length(CAST({literal} AS BLOB))"))
        .await
        .unwrap();

    assert_eq!(result.rows, vec![vec![Value::Text("a\0b".into()), Value::Int(3)]]);
}
