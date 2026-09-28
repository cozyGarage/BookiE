#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{Connection, OperationControl, QueryResult, Value};
use tokio_util::sync::CancellationToken;

pub async fn assert_array_contract(connection: &dyn Connection) {
    let control = OperationControl::new(CancellationToken::new(), None);
    let mut session = connection.open_session().await.unwrap();
    session
        .query_params_controlled("SET TIME ZONE 'Asia/Kathmandu'", &[], &control)
        .await
        .unwrap();
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
        (
            "date[]",
            "ARRAY['0002-12-31 BC','10000-01-01','infinity','-infinity',NULL]::date[]",
        ),
        ("time[]", "ARRAY['00:00:00','24:00:00','23:59:59.999999',NULL]::time[]"),
        (
            "timetz[]",
            "ARRAY['00:00:00+15:59:59','24:00:00-15:59:59',NULL]::timetz[]",
        ),
        (
            "timestamp[]",
            "ARRAY['0001-01-01 00:00:00.000001 BC','10000-01-01 23:59:59.999999','infinity','-infinity',NULL]::timestamp[]",
        ),
        (
            "timestamptz[]",
            "ARRAY['2024-11-03 01:30:00-04','2024-11-03 01:30:00-05','0001-01-01 12:34:56+00 BC','infinity','-infinity',NULL]::timestamptz[]",
        ),
        (
            "date[]",
            "'[0:1][-1:0]={{2000-01-01,NULL},{infinity,-infinity}}'::date[]",
        ),
        (
            "interval[]",
            "ARRAY['-1 month +2 days +0.000001 seconds','+1 month -2 days -0.000001 seconds','-9223372036854.775808 seconds',NULL]::interval[]",
        ),
    ] {
        let sql = format!("SELECT ({expression}) AS value, encode(array_send({expression}), 'hex') AS wire");
        let result = connection.query(&sql).await.unwrap();
        assert_array_value(kind, &result);
        crate::wire_round_trip::assert_wire_round_trip(
            session.as_mut(),
            &control,
            kind,
            &result.columns[0],
            &result.rows[0][0],
            &result.rows[0][1],
        )
        .await;
        let session_result = session.query_params_controlled(&sql, &[], &control).await.unwrap();
        assert_eq!(session_result.rows, result.rows, "{kind}: session");
        assert_eq!(session_result.columns[0].data_type, result.columns[0].data_type);
        let bound = session
            .query_params_controlled(
                &format!("SELECT encode(array_send($1::text::{kind}), 'hex')"),
                &result.rows[0][..1],
                &control,
            )
            .await
            .unwrap();
        assert_eq!(bound.rows[0][0], result.rows[0][1], "{kind}: non-UTC session import");
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_json_array_elements_are_explicitly_unsupported() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY['{\"a\": 1}'::jsonb, 'null'::jsonb]";
    let result = connection
        .query(&format!(
            "SELECT {expression} AS value, pg_typeof({expression})::text AS array_type, \
             array_to_json({expression})::text AS json_text, \
             array_to_json({expression})::jsonb = '[{{\"a\": 1}}, null]'::jsonb AS server_match"
        ))
        .await
        .unwrap();

    let value = &result.rows[0][0];
    assert!(matches!(value, Value::Undecodable(_)), "{value:?}");
    assert_eq!(result.rows[0][1], Value::Text("jsonb[]".into()));
    assert_eq!(result.rows[0][2], Value::Text("[{\"a\": 1},null]".into()));
    assert_eq!(result.rows[0][3], Value::Bool(true));
    assert!(tablepro_core::sql_literal::render_sql_literal("postgres", value).is_err());
    assert!(
        connection
            .query_params("SELECT $1", std::slice::from_ref(value))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_json_text_array_is_explicitly_unsupported() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let expression = "ARRAY['{\"a\": 1}'::json, 'null'::json]";
    let result = connection
        .query(&format!(
            "SELECT {expression} AS value, pg_typeof({expression})::text AS array_type, \
             array_to_json({expression})::text AS json_text, \
             array_to_json({expression})::jsonb = '[{{\"a\": 1}}, null]'::jsonb AS server_match"
        ))
        .await
        .unwrap();

    let value = &result.rows[0][0];
    assert!(matches!(value, Value::Undecodable(_)), "{value:?}");
    assert_eq!(result.rows[0][1], Value::Text("json[]".into()));
    assert_eq!(result.rows[0][2], Value::Text("[{\"a\": 1},null]".into()));
    assert_eq!(result.rows[0][3], Value::Bool(true));
    assert!(tablepro_core::sql_literal::render_sql_literal("postgres", value).is_err());
    assert!(
        connection
            .query_params("SELECT $1", std::slice::from_ref(value))
            .await
            .is_err()
    );
}

pub async fn assert_array_grid_edit(connection: &dyn Connection) {
    connection
        .execute("CREATE TABLE array_grid_edit (id integer PRIMARY KEY, value integer[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO array_grid_edit VALUES (1, ARRAY[1,2])")
        .await
        .unwrap();
    let mut row = connection.query("SELECT * FROM array_grid_edit").await.unwrap();
    row.columns[0].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "array_grid_edit",
        &row.columns,
        &[(1, Value::Text("{3,NULL,5}".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);
    let updated = connection
        .query("SELECT value = ARRAY[3,NULL,5]::integer[] FROM array_grid_edit WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(updated.rows, vec![vec![Value::Bool(true)]]);

    const TEXT_ARRAY: &str = r#"{"plain",NULL,"quote \" slash \\, comma"}"#;
    connection
        .execute("CREATE TABLE text_array_grid_edit (id integer PRIMARY KEY, value text[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO text_array_grid_edit VALUES (1, ARRAY['before']::text[])")
        .await
        .unwrap();
    let mut row = connection.query("SELECT * FROM text_array_grid_edit").await.unwrap();
    let id_index = row.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = row.columns.iter().position(|column| column.name == "value").unwrap();
    row.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "text_array_grid_edit",
        &row.columns,
        &[(value_index, Value::Text(TEXT_ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let elements = connection
        .query("SELECT ordinality, item IS NULL, item FROM text_array_grid_edit CROSS JOIN LATERAL unnest(text_array_grid_edit.value) WITH ORDINALITY AS element(item, ordinality) ORDER BY ordinality")
        .await
        .unwrap();
    assert_eq!(
        elements.rows,
        vec![
            vec![Value::Int(1), Value::Bool(false), Value::Text("plain".into())],
            vec![Value::Int(2), Value::Bool(true), Value::Null],
            vec![
                Value::Int(3),
                Value::Bool(false),
                Value::Text("quote \" slash \\, comma".into()),
            ],
        ]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_array_grid_edit_preserves_array_elements() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    assert_array_grid_edit(connection.as_ref()).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_numeric_array_grid_edit_preserves_elements() {
    const NUMERIC_ARRAY: &str =
        "{1234567890123456789012345678901234567890.12345678901234567890,1.2300,NaN,Infinity,-Infinity,NULL}";
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TABLE numeric_array_grid_edit (id integer PRIMARY KEY, value numeric[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO numeric_array_grid_edit VALUES (1, ARRAY[0]::numeric[])")
        .await
        .unwrap();
    let mut row = connection.query("SELECT * FROM numeric_array_grid_edit").await.unwrap();
    let id_index = row.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = row.columns.iter().position(|column| column.name == "value").unwrap();
    row.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "numeric_array_grid_edit",
        &row.columns,
        &[(value_index, Value::Text(NUMERIC_ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let elements = connection
        .query("SELECT ordinality, item IS NULL, item::text FROM numeric_array_grid_edit CROSS JOIN LATERAL unnest(numeric_array_grid_edit.value) WITH ORDINALITY AS element(item, ordinality) ORDER BY ordinality")
        .await
        .unwrap();
    assert_eq!(
        elements.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Bool(false),
                Value::Text("1234567890123456789012345678901234567890.12345678901234567890".into()),
            ],
            vec![Value::Int(2), Value::Bool(false), Value::Text("1.2300".into())],
            vec![Value::Int(3), Value::Bool(false), Value::Text("NaN".into())],
            vec![Value::Int(4), Value::Bool(false), Value::Text("Infinity".into())],
            vec![Value::Int(5), Value::Bool(false), Value::Text("-Infinity".into())],
            vec![Value::Int(6), Value::Bool(true), Value::Null],
        ]
    );
}

fn assert_array_value(kind: &str, result: &QueryResult) {
    let value = &result.rows[0][0];
    assert!(matches!(value, Value::Null | Value::Text(_)), "{kind}: {value:?}");
    let expected_type = if kind == "bpchar[]" {
        "CHAR[]".into()
    } else {
        kind.to_ascii_uppercase()
    };
    assert_eq!(result.columns[0].data_type, expected_type);
    let json = tablepro_core::export::row_to_json(&result.columns, &result.rows[0]);
    match value {
        Value::Null => assert!(json["value"].is_null()),
        Value::Text(text) => assert_eq!(json["value"].as_str(), Some(text.as_str())),
        other => panic!("unexpected array value: {other:?}"),
    }
}
