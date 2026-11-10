#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::Value;

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_column_type_change_does_not_poison_the_next_edit_of_the_same_statement() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE pa (id int PRIMARY KEY, v int)")
        .await
        .unwrap();
    let sql = "INSERT INTO pa (id, v) VALUES ($1, $2)";
    let first = conn.execute_params(sql, &[Value::Int(1), Value::Int(5)]).await;
    assert_eq!(first.unwrap().rows_affected, 1);
    conn.execute("ALTER TABLE pa ALTER COLUMN v TYPE text USING v::text")
        .await
        .unwrap();
    let second = conn
        .execute_params(sql, &[Value::Int(2), Value::Text("abc".into())])
        .await;
    assert_eq!(second.unwrap().rows_affected, 1, "insert after ALTER TYPE");
    let upd = "UPDATE pa SET v = $1 WHERE id = $2";
    let a = conn.execute_params(upd, &[Value::Int(9), Value::Int(1)]).await;
    assert_eq!(a.unwrap().rows_affected, 1);
    let after_int_into_text = conn.query("SELECT v FROM pa WHERE id = 1").await.unwrap();
    assert_eq!(
        after_int_into_text.rows.iter().next().unwrap()[0],
        Value::Text("9".into())
    );
    conn.execute("ALTER TABLE pa ALTER COLUMN v TYPE text USING v::text")
        .await
        .unwrap();
    let b = conn
        .execute_params(upd, &[Value::Text("x".into()), Value::Int(1)])
        .await;
    assert_eq!(b.unwrap().rows_affected, 1, "update text after ALTER");
    let rows = conn.query("SELECT id, v FROM pa ORDER BY id").await.unwrap();
    let rows: Vec<Vec<Value>> = rows.rows.iter().map(|row| row.to_vec()).collect();
    assert_eq!(
        rows,
        vec![
            vec![Value::Int(1), Value::Text("x".into())],
            vec![Value::Int(2), Value::Text("abc".into())],
        ]
    );
}
