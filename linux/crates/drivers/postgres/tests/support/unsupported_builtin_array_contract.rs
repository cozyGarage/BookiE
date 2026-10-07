#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::Value;

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_text_search_array_refusal_preserves_target_and_sibling_rows() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    connection
        .execute(
            "CREATE TABLE text_search_array_refusal \
             (id integer PRIMARY KEY, vectors tsvector[], queries tsquery[], sibling text NOT NULL)",
        )
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO text_search_array_refusal VALUES \
             (1, ARRAY[to_tsvector('simple', 'cats dogs'), ''::tsvector, NULL], \
                 ARRAY[to_tsquery('simple', 'cats & dogs'), ''::tsquery, NULL], 'target'), \
             (2, ARRAY[to_tsvector('simple', 'sibling')], \
                 ARRAY[to_tsquery('simple', 'sibling')], 'sibling')",
        )
        .await
        .unwrap();

    let source = connection
        .query(
            "SELECT vectors, queries, pg_typeof(vectors)::text, pg_typeof(queries)::text, \
             vectors::text, queries::text, array_to_json(vectors)::text, \
             array_to_json(queries)::text, encode(array_send(vectors), 'hex'), \
             encode(array_send(queries), 'hex') \
             FROM text_search_array_refusal WHERE id = 1",
        )
        .await
        .unwrap();
    let vector_refusal = source.rows[0][0].clone();
    let query_refusal = source.rows[0][1].clone();
    assert!(
        matches!(&vector_refusal, Value::Undecodable(name) if name.eq_ignore_ascii_case("TSVECTOR[]")),
        "tsvector[] must remain visibly unsupported: {vector_refusal:?}"
    );
    assert!(
        matches!(&query_refusal, Value::Undecodable(name) if name.eq_ignore_ascii_case("TSQUERY[]")),
        "tsquery[] must remain visibly unsupported: {query_refusal:?}"
    );
    assert_eq!(source.rows[0][2], Value::Text("tsvector[]".into()));
    assert_eq!(source.rows[0][3], Value::Text("tsquery[]".into()));
    assert_eq!(
        source.rows[0][6],
        Value::Text(r#"["'cats':1 'dogs':2","",null]"#.into())
    );
    assert_eq!(source.rows[0][7], Value::Text(r#"["'cats' & 'dogs'","",null]"#.into()));

    let before = connection
        .query(
            "SELECT id, vectors::text, queries::text, array_to_json(vectors)::text, \
             array_to_json(queries)::text, encode(array_send(vectors), 'hex'), \
             encode(array_send(queries), 'hex'), sibling \
             FROM text_search_array_refusal ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(before.rows.len(), 2);
    assert_eq!(before.rows[0][1], source.rows[0][4]);
    assert_eq!(before.rows[0][2], source.rows[0][5]);
    assert_eq!(before.rows[0][3], source.rows[0][6]);
    assert_eq!(before.rows[0][4], source.rows[0][7]);
    assert_eq!(before.rows[0][5], source.rows[0][8]);
    assert_eq!(before.rows[0][6], source.rows[0][9]);

    for refusal in [&vector_refusal, &query_refusal] {
        assert!(tablepro_core::sql_literal::render_sql_literal("postgres", refusal).is_err());
        assert!(
            connection
                .query_params("SELECT $1", std::slice::from_ref(refusal))
                .await
                .is_err()
        );
    }
    assert!(
        connection
            .execute_params(
                "UPDATE text_search_array_refusal SET vectors = $1 WHERE id = 1",
                std::slice::from_ref(&vector_refusal),
            )
            .await
            .is_err()
    );
    assert!(
        connection
            .execute_params(
                "UPDATE text_search_array_refusal SET queries = $1 WHERE id = 1",
                std::slice::from_ref(&query_refusal),
            )
            .await
            .is_err()
    );

    let after = connection
        .query(
            "SELECT id, vectors::text, queries::text, array_to_json(vectors)::text, \
             array_to_json(queries)::text, encode(array_send(vectors), 'hex'), \
             encode(array_send(queries), 'hex'), sibling \
             FROM text_search_array_refusal ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(after.rows, before.rows, "refused bindings changed stored values");
    assert_eq!(after.rows[0][7], Value::Text("target".into()));
    assert_eq!(after.rows[1][7], Value::Text("sibling".into()));
}
