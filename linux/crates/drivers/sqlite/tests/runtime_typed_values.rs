#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use drivers_sqlite::SqliteDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver, Value};

fn memory_options() -> ConnectOptions {
    ConnectOptions {
        database: ":memory:".into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn declared_boolean_and_temporal_values_keep_their_core_types() {
    let connection = SqliteDriver.connect(memory_options()).await.unwrap();
    connection
        .execute(
            "CREATE TABLE typed_values (\
                 id INTEGER PRIMARY KEY, enabled BOOLEAN, day DATE, clock TIME, \
                 moment DATETIME, legacy_moment TIMESTAMP\
             )",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO typed_values VALUES \
             (1, 1, '2026-10-08', '12:34:56.123456', \
              '2026-10-08 12:34:56.123456', '2026-10-08 12:34:56.123456'), \
             (2, 0, '2026-10-08', '12:34:56.123456', \
              '2026-10-08 12:34:56.123456', '2026-10-08 12:34:56.123456'), \
             (3, NULL, NULL, NULL, NULL, NULL)",
        )
        .await
        .unwrap();

    let result = connection
        .query(
            "SELECT id, enabled, typeof(enabled), day, typeof(day), \
                    clock, typeof(clock), moment, typeof(moment), \
                    legacy_moment, typeof(legacy_moment) \
             FROM typed_values ORDER BY id",
        )
        .await
        .unwrap();
    let date = NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
    let time = NaiveTime::from_hms_micro_opt(12, 34, 56, 123_456).unwrap();
    let datetime = NaiveDateTime::new(date, time);
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Bool(true),
                Value::Text("integer".into()),
                Value::Date(date),
                Value::Text("text".into()),
                Value::Time(time),
                Value::Text("text".into()),
                Value::DateTime(datetime),
                Value::Text("text".into()),
                Value::DateTime(datetime),
                Value::Text("text".into()),
            ],
            vec![
                Value::Int(2),
                Value::Bool(false),
                Value::Text("integer".into()),
                Value::Date(date),
                Value::Text("text".into()),
                Value::Time(time),
                Value::Text("text".into()),
                Value::DateTime(datetime),
                Value::Text("text".into()),
                Value::DateTime(datetime),
                Value::Text("text".into()),
            ],
            vec![
                Value::Int(3),
                Value::Null,
                Value::Text("null".into()),
                Value::Null,
                Value::Text("null".into()),
                Value::Null,
                Value::Text("null".into()),
                Value::Null,
                Value::Text("null".into()),
                Value::Null,
                Value::Text("null".into()),
            ],
        ]
    );
}

#[tokio::test]
async fn blob_affinity_values_are_decoded_by_their_runtime_storage_class() {
    let connection = SqliteDriver.connect(memory_options()).await.unwrap();
    connection
        .execute("CREATE TABLE flexible_blob (value BLOB)")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO flexible_blob VALUES ('42'), (42), (1.5), (X'00FF'), (NULL)")
        .await
        .unwrap();

    let result = connection
        .query("SELECT value, typeof(value) FROM flexible_blob")
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![Value::Text("42".into()), Value::Text("text".into())],
            vec![Value::Int(42), Value::Text("integer".into())],
            vec![Value::Float(1.5), Value::Text("real".into())],
            vec![Value::Bytes(vec![0, 255]), Value::Text("blob".into())],
            vec![Value::Null, Value::Text("null".into())],
        ]
    );
}

#[tokio::test]
async fn declared_enum_values_follow_sqlite_numeric_affinity_and_keep_runtime_kinds() {
    let connection = SqliteDriver.connect(memory_options()).await.unwrap();
    connection.execute("CREATE TABLE enum_like (value ENUM)").await.unwrap();
    connection
        .execute("INSERT INTO enum_like VALUES ('queued'), ('3.5'), ('7'), (X'00FF'), (NULL)")
        .await
        .unwrap();

    let result = connection
        .query("SELECT value, typeof(value), quote(value) FROM enum_like ORDER BY rowid")
        .await
        .unwrap();
    assert_eq!(result.columns[0].data_type, "ENUM");
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Text("queued".into()),
                Value::Text("text".into()),
                Value::Text("'queued'".into())
            ],
            vec![Value::Float(3.5), Value::Text("real".into()), Value::Text("3.5".into())],
            vec![Value::Int(7), Value::Text("integer".into()), Value::Text("7".into())],
            vec![
                Value::Bytes(vec![0, 255]),
                Value::Text("blob".into()),
                Value::Text("X'00FF'".into())
            ],
            vec![Value::Null, Value::Text("null".into()), Value::Text("NULL".into())],
        ]
    );
}

#[tokio::test]
async fn invalid_declared_temporal_values_remain_text() {
    let connection = SqliteDriver.connect(memory_options()).await.unwrap();
    connection
        .execute("CREATE TABLE malformed_temporals (day DATE, clock TIME, moment DATETIME, legacy_moment TIMESTAMP)")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO malformed_temporals VALUES \
             ('2026-13-40', '25:61:62', 'not-a-datetime', '2026-13-40 25:61:62')",
        )
        .await
        .unwrap();

    let result = connection
        .query("SELECT day, clock, moment, legacy_moment FROM malformed_temporals")
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![vec![
            Value::Text("2026-13-40".into()),
            Value::Text("25:61:62".into()),
            Value::Text("not-a-datetime".into()),
            Value::Text("2026-13-40 25:61:62".into()),
        ]]
    );
}
