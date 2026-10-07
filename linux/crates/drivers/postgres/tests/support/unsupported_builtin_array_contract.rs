#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::Value;

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_tsvector_array_refusal_preserves_target_and_sibling_rows() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    connection
        .execute(
            "CREATE TABLE tsvector_array_refusal \
             (id integer PRIMARY KEY, value tsvector[], sibling text NOT NULL)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO tsvector_array_refusal VALUES \
             (1, ARRAY[to_tsvector('simple', 'cats dogs'), ''::tsvector, NULL], 'target'), \
             (2, ARRAY[to_tsvector('simple', 'sibling')], 'sibling')",
        )
        .await
        .unwrap();

    let source = connection
        .query(
            "SELECT value, pg_typeof(value)::text, value::text, \
             array_to_json(value)::text, encode(array_send(value), 'hex') \
             FROM tsvector_array_refusal WHERE id = 1",
        )
        .await
        .unwrap();
    let refusal = source.rows[0][0].clone();
    assert!(
        matches!(&refusal, Value::Undecodable(name) if name.eq_ignore_ascii_case("TSVECTOR[]")),
        "tsvector[] must remain visibly unsupported: {refusal:?}"
    );
    assert_eq!(source.rows[0][1], Value::Text("tsvector[]".into()));
    assert_eq!(
        source.rows[0][3],
        Value::Text(r#"["'cats':1 'dogs':2","",null]"#.into())
    );
    let before = connection
        .query(
            "SELECT id, value::text, array_to_json(value)::text, \
             encode(array_send(value), 'hex'), sibling \
             FROM tsvector_array_refusal ORDER BY id",
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
            .execute_params("UPDATE tsvector_array_refusal SET value = $1 WHERE id = 1", &[refusal])
            .await
            .is_err()
    );

    let after = connection
        .query(
            "SELECT id, value::text, array_to_json(value)::text, \
             encode(array_send(value), 'hex'), sibling \
             FROM tsvector_array_refusal ORDER BY id",
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
