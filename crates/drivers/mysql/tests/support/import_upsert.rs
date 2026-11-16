use std::str::FromStr;

use super::{connect, start_mariadb, start_mysql};
use bookie_core::import::{CsvImportOptions, ImportTarget, WriteMode, build_insert_plan, read_csv};
use bookie_core::{Connection, DatabaseDriver, Value};
use drivers_mysql::MysqlDriver;
use rust_decimal::Decimal;

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_import_upsert_updates_the_existing_row_and_inserts_the_new_one() {
    let (_container, opts) = start_mysql().await;
    let conn = connect(opts).await;
    assert_upsert_round_trip(conn.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_import_upsert_updates_the_existing_row_and_inserts_the_new_one() {
    let (_container, opts) = start_mariadb().await;
    let conn = connect(opts).await;
    assert_upsert_round_trip(conn.as_ref()).await;
}

async fn assert_upsert_round_trip(conn: &dyn Connection) {
    conn.execute(
        "CREATE TABLE import_upsert (id BIGINT PRIMARY KEY, name VARCHAR(40) NOT NULL, amount DECIMAL(10,2), seen DATETIME(3) NULL)",
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO import_upsert VALUES (1, 'original', 1.00, '2020-01-01 00:00:00.000'), (2, 'untouched', 9.99, NULL)",
    )
    .await
    .unwrap();
    let options = CsvImportOptions::default();
    let sheet = read_csv(
        b"id,name,amount,seen\n1,renamed,2.50,2024-05-06 07:08:09.123\n3,fresh,7.25,2024-06-01 00:00:00.000\n",
        &options,
        None,
    )
    .unwrap();
    let columns = conn.fetch_columns(None, "import_upsert").await.unwrap();
    let target = ImportTarget {
        driver_id: "mysql",
        capabilities: MysqlDriver.capabilities(),
        schema: None,
        table: "import_upsert",
        columns: &columns,
        mapping: &[Some(0), Some(1), Some(2), Some(3)],
    };
    let plan = build_insert_plan(&target, &sheet, &options).unwrap();
    let plan = WriteMode::UpdateOnConflict(vec!["id".into()])
        .apply(plan, &target)
        .unwrap();
    assert!(plan.statement.contains("ON DUPLICATE KEY UPDATE"));

    let statements: Vec<(String, Vec<Value>)> = plan
        .rows
        .iter()
        .map(|row| (plan.statement.clone(), row.clone()))
        .collect();
    conn.execute_in_transaction(&statements).await.unwrap();

    let result = conn
        .query("SELECT id, name, amount, seen FROM import_upsert ORDER BY id")
        .await
        .unwrap();
    let seen =
        |text: &str| Value::DateTime(chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f").unwrap());
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("renamed".into()),
                Value::Decimal(Decimal::from_str("2.50").unwrap()),
                seen("2024-05-06 07:08:09.123"),
            ],
            vec![
                Value::Int(2),
                Value::Text("untouched".into()),
                Value::Decimal(Decimal::from_str("9.99").unwrap()),
                Value::Null,
            ],
            vec![
                Value::Int(3),
                Value::Text("fresh".into()),
                Value::Decimal(Decimal::from_str("7.25").unwrap()),
                seen("2024-06-01 00:00:00.000"),
            ],
        ]
    );
}
