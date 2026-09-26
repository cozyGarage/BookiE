#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{Connection, OperationControl, QueryResult, Value};
use tokio_util::sync::CancellationToken;

pub async fn assert_array_contract(connection: &dyn Connection) {
    let control = OperationControl::new(CancellationToken::new(), None);
    let mut session = connection.open_session().await.unwrap();
    for (kind, expression) in [
        ("int4[]", "NULL::int4[]"),
        ("int4[]", "ARRAY[]::int4[]"),
        ("int4[]", "'{{{{{{1,NULL}}}}}}'::int4[]"),
        ("name[]", "ARRAY['alpha','',NULL]::name[]"),
        ("oid[]", "ARRAY[0,4294967295,NULL]::oid[]"),
        ("int4[]", "ARRAY[1,NULL,-2147483648,2147483647]"),
        ("int2[]", "ARRAY[-32768,0,32767]::int2[]"),
        ("int8[]", "ARRAY[-9223372036854775808,9223372036854775807]::int8[]"),
        ("int4[]", "'[0:1][-2:0]={{1,NULL,3},{4,5,6}}'::int4[]"),
        (
            "text[]",
            "ARRAY[NULL,'NULL','null','','{}','a,b',' leading ', '漢字 😀', $$a\"b$$, $$a\\b$$, E'line\nnext', $$x'); DROP TABLE t; --$$]",
        ),
        ("text[]", "ARRAY[['a',NULL],['','NULL']]"),
        ("varchar[]", "ARRAY['x','',NULL]::varchar[]"),
        ("bpchar[]", "ARRAY['x',' y']::char(3)[]"),
        (
            "numeric[]",
            "ARRAY[1234567890123456789012345678901234567890,0.123456789012345678901234567891,1.2300,NULL,'NaN'::numeric,'Infinity'::numeric]",
        ),
        ("bool[]", "ARRAY[true,false,NULL]"),
        (
            "float4[]",
            "ARRAY[0.1,'-0','NaN','Infinity','-Infinity',NULL]::float4[]",
        ),
        (
            "float8[]",
            "ARRAY[1e-200,1.0000000000000002,'-0','NaN','Infinity',NULL]::float8[]",
        ),
        ("uuid[]", "ARRAY['12345678-1234-5678-90ab-1234567890ab',NULL]::uuid[]"),
        ("bytea[]", "ARRAY[decode('00ff275c','hex'),decode('','hex'),NULL]"),
    ] {
        let sql = format!("SELECT ({expression}) AS value, encode(array_send({expression}), 'hex') AS wire");
        let result = connection.query(&sql).await.unwrap();
        assert_round_trip(connection, kind, &result).await;
        let session_result = session.query_params_controlled(&sql, &[], &control).await.unwrap();
        assert_eq!(session_result.rows, result.rows, "{kind}: session");
        assert_eq!(session_result.columns[0].data_type, result.columns[0].data_type);
    }
}

async fn assert_round_trip(connection: &dyn Connection, kind: &str, result: &QueryResult) {
    let value = &result.rows[0][0];
    assert!(matches!(value, Value::Null | Value::Text(_)), "{kind}: {value:?}");
    let expected_type = if kind == "bpchar[]" {
        "CHAR[]".into()
    } else {
        kind.to_ascii_uppercase()
    };
    assert_eq!(result.columns[0].data_type, expected_type);
    let expected = &result.rows[0][1];
    let literal = tablepro_core::sql_literal::render_sql_literal("postgres", value).unwrap();
    let sql = format!("SELECT encode(array_send({literal}::{kind}), 'hex')");
    let exported = connection.query(&sql).await.unwrap();
    assert_eq!(&exported.rows[0][0], expected, "{kind}: exported");
    let sql = format!("SELECT encode(array_send($1::text::{kind}), 'hex')");
    let bound = connection
        .query_params(&sql, std::slice::from_ref(value))
        .await
        .unwrap();
    assert_eq!(&bound.rows[0][0], expected, "{kind}: bound");
    connection
        .execute("DROP TABLE IF EXISTS array_export_target")
        .await
        .unwrap();
    connection
        .execute(&format!("CREATE TABLE array_export_target (value {kind})"))
        .await
        .unwrap();
    let insert = tablepro_core::sql_literal::build_insert_literal(
        "postgres",
        None,
        "array_export_target",
        &result.columns[..1],
        std::slice::from_ref(value),
    )
    .unwrap();
    connection.execute(&insert).await.unwrap();
    let inserted = connection
        .query("SELECT encode(array_send(value), 'hex') FROM array_export_target")
        .await
        .unwrap();
    assert_eq!(&inserted.rows[0][0], expected, "{kind}: INSERT export");
    let json = tablepro_core::export::row_to_json(&result.columns, &result.rows[0]);
    match value {
        Value::Null => assert!(json["value"].is_null()),
        Value::Text(text) => assert_eq!(json["value"].as_str(), Some(text.as_str())),
        other => panic!("unexpected array value: {other:?}"),
    }
}
