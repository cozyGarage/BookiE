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
        (
            "CAST([tuple(toDecimal128('12345678901234567890.123456789', 9), toDateTime64('2026-09-30 12:34:56.123456789', 9, 'UTC')), tuple(toDecimal128('-0.000000001', 9), toDateTime64('1999-12-31 23:59:59.000000001', 9, 'UTC'))] AS Array(Tuple(Decimal(38, 9), DateTime64(9, 'UTC'))))",
            "Array(Tuple(Decimal(38, 9), DateTime64(9, 'UTC')))",
        ),
        (
            "CAST([tuple('precise', CAST([toDecimal128('12345678901234567890.123456789', 9), CAST(NULL AS Nullable(Decimal(38, 9)))] AS Array(Nullable(Decimal(38, 9))))), tuple('empty', CAST([] AS Array(Nullable(Decimal(38, 9)))))] AS Array(Tuple(String, Array(Nullable(Decimal(38, 9))))))",
            "Array(Tuple(String, Array(Nullable(Decimal(38, 9)))))",
        ),
        (
            "CAST(map('measurements', [tuple('upper', toDecimal128('12345678901234567890.123456789', 9)), tuple('missing', CAST(NULL AS Nullable(Decimal(38, 9))))], 'empty', CAST([] AS Array(Tuple(String, Nullable(Decimal(38, 9)))))) AS Map(String, Array(Tuple(String, Nullable(Decimal(38, 9))))))",
            "Map(String, Array(Tuple(String, Nullable(Decimal(38, 9)))))",
        ),
        (
            "tuple('measurements', CAST(map('upper', CAST(toDecimal128('12345678901234567890.123456789', 9) AS Nullable(Decimal(38, 9))), 'missing', CAST(NULL AS Nullable(Decimal(38, 9)))) AS Map(String, Nullable(Decimal(38, 9)))))",
            "Tuple(String, Map(String, Nullable(Decimal(38, 9))))",
        ),
        (
            "CAST([map('upper', CAST(toDecimal128('12345678901234567890.123456789', 9) AS Nullable(Decimal(38, 9))), 'missing', CAST(NULL AS Nullable(Decimal(38, 9))))] AS Array(Map(String, Nullable(Decimal(38, 9)))))",
            "Array(Map(String, Nullable(Decimal(38, 9))))",
        ),
        (
            "CAST(map(toUInt8(1), CAST(toUInt128('18446744073709551616') AS Nullable(UInt128)), toUInt8(255), CAST(NULL AS Nullable(UInt128))) AS Map(UInt8, Nullable(UInt128)))",
            "Map(UInt8, Nullable(UInt128))",
        ),
        (
            "CAST(map(toUInt8(1), [CAST(toUInt128('18446744073709551616') AS Nullable(UInt128)), CAST(NULL AS Nullable(UInt128))], toUInt8(255), CAST([] AS Array(Nullable(UInt128)))) AS Map(UInt8, Array(Nullable(UInt128))))",
            "Map(UInt8, Array(Nullable(UInt128)))",
        ),
        (
            "CAST([map(toUInt8(1), CAST(toUInt128('18446744073709551616') AS Nullable(UInt128)), toUInt8(255), CAST(NULL AS Nullable(UInt128)))] AS Array(Map(UInt8, Nullable(UInt128))))",
            "Array(Map(UInt8, Nullable(UInt128)))",
        ),
        (
            "CAST([tuple('wide', map(toUInt8(1), CAST(toUInt128('18446744073709551616') AS Nullable(UInt128)), toUInt8(255), CAST(NULL AS Nullable(UInt128))))] AS Array(Tuple(String, Map(UInt8, Nullable(UInt128)))))",
            "Array(Tuple(String, Map(UInt8, Nullable(UInt128))))",
        ),
        (
            "CAST(map(toUInt8(1), [tuple('wide', CAST(toUInt128('18446744073709551616') AS Nullable(UInt128))), tuple('missing', CAST(NULL AS Nullable(UInt128)))], toUInt8(255), CAST([] AS Array(Tuple(String, Nullable(UInt128))))) AS Map(UInt8, Array(Tuple(String, Nullable(UInt128)))))",
            "Map(UInt8, Array(Tuple(String, Nullable(UInt128))))",
        ),
    ];

    for (index, (expression, expected_type)) in cases.into_iter().enumerate() {
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
            Value::Json(oracle.clone()),
            "nested {expected_type} value differs from the native server JSON oracle"
        );
        let json_output: serde_json::Value =
            serde_json::from_str(&tablepro_core::export::render_json(&source.columns, &source.rows)).unwrap();
        assert_eq!(
            json_output[0]["value"], oracle,
            "JSON export must preserve nested {expected_type}"
        );
        let csv_output = tablepro_core::export::render_csv(
            &source.columns,
            &source.rows,
            &tablepro_core::export::CsvOptions::default(),
        );
        let csv_sheet = tablepro_core::import::read_csv(
            csv_output.as_bytes(),
            &tablepro_core::import::CsvImportOptions::default(),
            None,
        )
        .unwrap();
        let value_index = csv_sheet.headers.iter().position(|name| name == "value").unwrap();
        let csv_json: serde_json::Value = serde_json::from_str(&csv_sheet.rows[0][value_index]).unwrap();
        assert_eq!(csv_json, oracle, "CSV export must preserve nested {expected_type} JSON");
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

        let table = format!("nested_json_edit_refusal_{index}");
        connection
            .query(&format!("DROP TABLE IF EXISTS {table}"))
            .await
            .unwrap();
        connection
            .query(&format!(
                "CREATE TABLE {table} (id UInt8, value {expected_type}) ENGINE = MergeTree ORDER BY id"
            ))
            .await
            .unwrap();
        connection
            .query(&format!("INSERT INTO {table} SELECT 1, {expression}"))
            .await
            .unwrap();

        let columns = connection.fetch_columns(None, &table).await.unwrap();
        let id_index = columns.iter().position(|column| column.name == "id").unwrap();
        let value_index = columns.iter().position(|column| column.name == "value").unwrap();
        assert!(columns[id_index].primary_key, "{table} must expose its key for an edit");
        let before = connection
            .query(&format!(
                "SELECT id, value, toTypeName(value), toJSONString(value) FROM {table}"
            ))
            .await
            .unwrap();
        assert_eq!(before.rows.len(), 1);
        assert_eq!(before.rows[0][2], Value::Text(expected_type.into()));
        assert_eq!(before.rows[0][1], source.rows[0][0]);

        let update = tablepro_core::sql_dialect::build_keyed_update(
            "clickhouse",
            None,
            &table,
            &columns,
            &[(value_index, before.rows[0][value_index].clone())],
            &[before.rows[0][id_index].clone()],
        )
        .unwrap();
        let edit_result = connection.execute_in_transaction(&[update]).await;
        assert!(
            matches!(
                &edit_result,
                Err(tablepro_core::DriverError::Transaction {
                    statement_index: 0,
                    source,
                }) if matches!(source.as_ref(), tablepro_core::DriverError::Unsupported(_))
            ),
            "nested {expected_type} grid edit must be refused before changing the row, got {edit_result:?}"
        );
        let after = connection
            .query(&format!(
                "SELECT id, value, toTypeName(value), toJSONString(value) FROM {table}"
            ))
            .await
            .unwrap();
        assert_eq!(
            after.rows, before.rows,
            "refusing a nested {expected_type} edit must preserve the stored native value"
        );
        connection.query(&format!("DROP TABLE {table}")).await.unwrap();
    }
}
