#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::Value;

use crate::{connect, start_pg};

const COLUMNS: [&str; 10] = ["n", "jb", "d", "u", "b", "num", "t", "ts", "ip", "j"];

#[tokio::test]
#[ignore = "requires docker"]
async fn a_null_parameter_takes_the_type_of_the_column_it_is_bound_to() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    conn.execute(
        "CREATE TABLE nulls (id int PRIMARY KEY, n int, jb jsonb, d date, u uuid, b boolean, num numeric, \
         t text, ts timestamptz, ip inet, j json)",
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO nulls VALUES (1, 5, '{}', '2026-01-01', gen_random_uuid(), true, 1.5, 'a', now(), '10.0.0.1', '{}')",
    )
    .await
    .unwrap();

    for column in COLUMNS {
        let set_null = conn
            .execute_params(&format!("UPDATE nulls SET {column} = $1 WHERE id = 1"), &[Value::Null])
            .await;
        assert_eq!(set_null.unwrap().rows_affected, 1, "setting {column} to NULL");
        if column == "j" {
            continue;
        }
        let compared = conn
            .execute_params(
                &format!("UPDATE nulls SET t = 'x' WHERE id = 1 AND {column} IS NOT DISTINCT FROM $1"),
                &[Value::Null],
            )
            .await;
        assert_eq!(
            compared.unwrap().rows_affected,
            1,
            "{column} IS NOT DISTINCT FROM NULL must match the row"
        );
    }

    let inserted = conn
        .execute_params(
            "INSERT INTO nulls (id, n, jb, d, u, ip) VALUES (2, $1, $2, $3, $4, $5)",
            &[Value::Null, Value::Null, Value::Null, Value::Null, Value::Null],
        )
        .await;
    assert_eq!(inserted.unwrap().rows_affected, 1, "inserting NULLs");

    let first = conn
        .query("SELECT n, jb, d, u, b, num, t, ts, ip, j FROM nulls WHERE id = 1")
        .await
        .unwrap();
    let first: Vec<Value> = first.rows.iter().next().unwrap().to_vec();
    for (column, value) in COLUMNS.iter().zip(&first) {
        if *column == "t" {
            assert_eq!(value, &Value::Text("x".into()), "t was marked by the NULL comparisons");
        } else {
            assert_eq!(value, &Value::Null, "{column} must be NULL");
        }
    }
    let second = conn
        .query("SELECT n, jb, d, u, ip FROM nulls WHERE id = 2")
        .await
        .unwrap();
    assert!(
        second
            .rows
            .iter()
            .next()
            .unwrap()
            .iter()
            .all(|value| *value == Value::Null)
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_null_in_an_ambiguous_placeholder_still_runs() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    for (sql, expected) in [
        ("SELECT $1 IS NULL AS r", Value::Bool(true)),
        ("SELECT $1 AS r", Value::Null),
    ] {
        let result = conn.query_params(sql, &[Value::Null]).await.unwrap();
        assert_eq!(result.rows.iter().next().unwrap()[0], expected, "{sql}");
    }
}
