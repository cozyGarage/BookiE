#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::Value;

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_domain_over_bytea_array_preserves_binary_values_and_wire_bytes() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    let schema = "domain_bytea_array";
    connection.execute(&format!("CREATE SCHEMA {schema}")).await.unwrap();
    connection
        .execute(&format!("CREATE DOMAIN {schema}.payload AS bytea"))
        .await
        .unwrap();

    let populated = format!(
        "ARRAY[decode('00ff275c','hex')::{schema}.payload, \
         decode('','hex')::{schema}.payload, NULL::{schema}.payload]"
    );
    let result = connection
        .query(&format!(
            "SELECT {populated}, NULL::{schema}.payload[], ARRAY[]::{schema}.payload[]"
        ))
        .await
        .unwrap();
    assert_eq!(
        result
            .columns
            .iter()
            .map(|column| column.data_type.as_str())
            .collect::<Vec<_>>(),
        [
            "domain_bytea_array.payload[]",
            "domain_bytea_array.payload[]",
            "domain_bytea_array.payload[]",
        ]
    );
    assert_eq!(result.rows[0][1], Value::Null);
    assert_eq!(result.rows[0][2], Value::Text("{}".into()));

    let native = connection
        .query(&format!(
            "SELECT pg_typeof(items)::text, items::text, array_to_json(items)::text, \
                    encode(array_send(items), 'hex') \
             FROM (SELECT {populated} AS items) AS source"
        ))
        .await
        .unwrap();
    assert_eq!(native.rows[0][0], Value::Text(format!("{schema}.payload[]")));
    assert_eq!(result.rows[0][0], native.rows[0][1]);

    let elements = connection
        .query(&format!(
            "SELECT ord::bigint, encode(element, 'hex') \
             FROM unnest({populated}) WITH ORDINALITY AS items(element, ord) ORDER BY ord"
        ))
        .await
        .unwrap();
    assert_eq!(
        elements.rows,
        vec![
            vec![Value::Int(1), Value::Text("00ff275c".into())],
            vec![Value::Int(2), Value::Text(String::new())],
            vec![Value::Int(3), Value::Null],
        ]
    );

    let bound = connection
        .query_params(
            &format!(
                "SELECT pg_typeof($1::text::{schema}.payload[])::text, \
                        array_to_json($1::text::{schema}.payload[])::text, \
                        encode(array_send($1::text::{schema}.payload[]), 'hex')"
            ),
            std::slice::from_ref(&result.rows[0][0]),
        )
        .await
        .unwrap();
    assert_eq!(bound.rows[0][0], native.rows[0][0]);
    assert_eq!(bound.rows[0][1], native.rows[0][2]);
    assert_eq!(bound.rows[0][2], native.rows[0][3]);

    for (value, expression) in [
        (&result.rows[0][1], format!("NULL::{schema}.payload[]")),
        (&result.rows[0][2], format!("ARRAY[]::{schema}.payload[]")),
    ] {
        let oracle = connection
            .query(&format!(
                "SELECT pg_typeof({expression})::text, array_to_json({expression})::text, \
                        encode(array_send({expression}), 'hex')"
            ))
            .await
            .unwrap();
        let rebound = connection
            .query_params(
                &format!(
                    "SELECT pg_typeof($1::text::{schema}.payload[])::text, \
                            array_to_json($1::text::{schema}.payload[])::text, \
                            encode(array_send($1::text::{schema}.payload[]), 'hex')"
                ),
                std::slice::from_ref(value),
            )
            .await
            .unwrap();
        assert_eq!(rebound.rows[0], oracle.rows[0]);
    }
}
