use super::clickhouse::{connect, start_clickhouse};
use tablepro_core::Value;

#[tokio::test]
#[ignore = "requires docker"]
async fn string_and_fixed_string_results_preserve_non_utf8_bytes() {
    let (_container, options) = start_clickhouse().await;
    let conn = connect(options).await;
    conn.execute(
        "CREATE TABLE non_utf8_strings (
            string_value String,
            fixed_value FixedString(2)
        ) ENGINE = Memory",
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO non_utf8_strings VALUES (unhex('ff'), unhex('ff00'))")
        .await
        .unwrap();

    let result = conn
        .query("SELECT string_value, fixed_value FROM non_utf8_strings")
        .await
        .unwrap();

    assert_eq!(
        result.rows,
        vec![vec![Value::Bytes(vec![0xff]), Value::Bytes(vec![0xff, 0x00])]]
    );
}
