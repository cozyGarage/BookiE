#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::Value;

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_range_array_refusal_preserves_target_and_sibling_rows() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    connection
        .execute(
            "CREATE TABLE range_array_refusal \
             (id integer PRIMARY KEY, value int4range[], sibling text NOT NULL)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO range_array_refusal VALUES \
             (1, ARRAY[int4range(1, 4), int4range(8, NULL, '(]'), 'empty'::int4range, NULL], 'target'), \
             (2, ARRAY[int4range(20, 25)], 'sibling')",
        )
        .await
        .unwrap();

    let source = connection
        .query(
            "SELECT value, pg_typeof(value)::text, value::text, \
             array_to_json(value)::text, encode(array_send(value), 'hex') \
             FROM range_array_refusal WHERE id = 1",
        )
        .await
        .unwrap();
    let refusal = source.rows[0][0].clone();
    assert!(
        matches!(&refusal, Value::Undecodable(name) if name.eq_ignore_ascii_case("INT4RANGE[]")),
        "int4range[] must remain visibly unsupported: {refusal:?}"
    );
    assert_eq!(source.rows[0][1], Value::Text("int4range[]".into()));
    assert_eq!(
        source.rows[0][3],
        Value::Text(r#"["[1,4)","[9,)","empty",null]"#.into())
    );
    let before = connection
        .query(
            "SELECT id, value::text, array_to_json(value)::text, \
             encode(array_send(value), 'hex'), sibling \
             FROM range_array_refusal ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(before.rows.len(), 2);
    assert_eq!(before.rows[0][1], source.rows[0][2]);
    assert_eq!(before.rows[0][2], source.rows[0][3]);
    assert_eq!(before.rows[0][3], source.rows[0][4]);

    assert!(tablepro_core::sql_literal::render_sql_literal("postgres", &refusal).is_err());
    assert!(
        connection
            .query_params("SELECT $1", std::slice::from_ref(&refusal))
            .await
            .is_err()
    );
    assert!(
        connection
            .execute_params("UPDATE range_array_refusal SET value = $1 WHERE id = 1", &[refusal])
            .await
            .is_err()
    );

    let after = connection
        .query(
            "SELECT id, value::text, array_to_json(value)::text, \
             encode(array_send(value), 'hex'), sibling \
             FROM range_array_refusal ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(after.rows, before.rows, "refused binding changed stored values");
    assert_eq!(after.rows[0][1], source.rows[0][2]);
    assert_eq!(after.rows[0][2], source.rows[0][3]);
    assert_eq!(after.rows[0][3], source.rows[0][4]);
    assert_eq!(after.rows[0][4], Value::Text("target".into()));
    assert_eq!(after.rows[1][4], Value::Text("sibling".into()));
}
