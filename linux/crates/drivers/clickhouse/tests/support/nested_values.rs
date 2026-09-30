use super::clickhouse::{connect, start_clickhouse};
use tablepro_core::Value;

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_nested_collections_keep_exact_json_and_refuse_lossy_consumers() {
    let (_container, opts) = start_clickhouse().await;
    let connection = connect(opts).await;
    let cases = [
        (
            "CAST([toUInt128('18446744073709551616'), CAST(NULL AS Nullable(UInt128))] AS Array(Nullable(UInt128)))",
            "Array(Nullable(UInt128))",
        ),
        (
            "CAST(map('wide', toUInt128('18446744073709551616')) AS Map(String, UInt128))",
            "Map(String, UInt128)",
        ),
        (
            "tuple('wide', toUInt128('340282366920938463463374607431768211455'))",
            "Tuple(String, UInt128)",
        ),
        (
            "CAST([map('wide', toUInt128('18446744073709551616'))] AS Array(Map(String, UInt128)))",
            "Array(Map(String, UInt128))",
        ),
        (
            "CAST(map('numbers', [toUInt128('1'), toUInt128('340282366920938463463374607431768211455')]) AS Map(String, Array(UInt128)))",
            "Map(String, Array(UInt128))",
        ),
        (
            "tuple('series', CAST([toUInt128('1'), CAST(NULL AS Nullable(UInt128))] AS Array(Nullable(UInt128))))",
            "Tuple(String, Array(Nullable(UInt128)))",
        ),
        (
            "CAST(map('pair', tuple('wide', toUInt128('18446744073709551616'))) AS Map(String, Tuple(String, UInt128)))",
            "Map(String, Tuple(String, UInt128))",
        ),
    ];

    for (expression, expected_type) in cases {
        let source = connection
            .query(&format!(
                "SELECT {expression} AS value, toTypeName(value) AS native_type, toJSONString(value) AS exact_json"
            ))
            .await
            .unwrap();
        assert_eq!(source.rows.len(), 1);
        assert_eq!(source.rows[0][1], Value::Text(expected_type.into()));
        let exact_json = match &source.rows[0][2] {
            Value::Text(value) => value,
            other => panic!("ClickHouse toJSONString oracle was not text: {other:?}"),
        };
        let oracle: serde_json::Value = serde_json::from_str(exact_json).unwrap();
        assert_eq!(
            source.rows[0][0],
            Value::Json(oracle),
            "nested {expected_type} value differs from the native server JSON oracle"
        );
        assert_eq!(
            tablepro_core::sql_literal::render_sql_literal("clickhouse", &source.rows[0][0]),
            Err(tablepro_core::sql_literal::LiteralError::Unsupported),
            "nested {expected_type} without type metadata must be refused by SQL export"
        );

        let bound_result = connection
            .query_params(
                &format!("SELECT CAST(? AS {expected_type}) AS value"),
                &[source.rows[0][0].clone()],
            )
            .await;
        assert!(
            bound_result.is_err(),
            "nested {expected_type} without type metadata must not bind as a lossy string"
        );
    }
}
