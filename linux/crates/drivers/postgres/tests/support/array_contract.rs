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

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_custom_enum_array_projection_is_rejected() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TYPE value_contract_array_enum AS ENUM ('NULL', '東京', 'o''brien')")
        .await
        .unwrap();
    let expression = "ARRAY['NULL'::value_contract_array_enum, \
        '東京'::value_contract_array_enum, \
        'o''brien'::value_contract_array_enum, NULL]";
    let result = connection
        .query(&format!(
            "SELECT pg_typeof(value)::text AS array_type, value::text AS exact_text, \
             array_to_json(value)::jsonb = \
               '[\"NULL\",\"東京\",\"o''brien\",null]'::jsonb AS server_match \
             FROM (SELECT {expression} AS value) source"
        ))
        .await
        .unwrap();

    assert_eq!(result.rows[0][0], Value::Text("value_contract_array_enum[]".into()));
    assert_eq!(result.rows[0][1], Value::Text(r#"{"NULL",東京,o'brien,NULL}"#.into()));
    assert_eq!(result.rows[0][2], Value::Bool(true));
    let direct_projection = connection
        .query(&format!("SELECT {expression} AS value"))
        .await
        .expect_err("SQLx currently fails decoding custom enum-array metadata");
    let error = format!("{direct_projection:?}");
    assert!(error.contains("enum_labels"), "{error}");
    assert!(error.contains("unexpected null"), "{error}");
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

    assert_bool_array_grid_edit(connection).await;
    assert_bytea_array_grid_edit(connection).await;
    assert_uuid_array_grid_edit(connection).await;
    assert_timestamptz_array_grid_edit(connection).await;
}

async fn assert_bool_array_grid_edit(connection: &dyn Connection) {
    connection
        .execute("CREATE TABLE bool_array_grid_edit (id integer PRIMARY KEY, value boolean[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO bool_array_grid_edit VALUES (1, ARRAY[true]::boolean[]), (2, ARRAY[false]::boolean[])")
        .await
        .unwrap();
    let mut rows = connection
        .query("SELECT * FROM bool_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(value), 'hex') FROM bool_array_grid_edit WHERE id = 2")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let id_index = rows.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = rows.columns.iter().position(|column| column.name == "value").unwrap();
    rows.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "bool_array_grid_edit",
        &rows.columns,
        &[(value_index, Value::Text("{true,false,NULL}".into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query("SELECT ARRAY[true,false,NULL]::boolean[]::text, encode(array_send(ARRAY[true,false,NULL]::boolean[]), 'hex')")
        .await
        .unwrap();
    let actual = connection
        .query("SELECT id, value::text, encode(array_send(value), 'hex') FROM bool_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    assert_eq!(actual.rows[0][1], expected.rows[0][0]);
    assert_eq!(actual.rows[0][2], expected.rows[0][1]);
    assert_eq!(actual.rows[1][0], Value::Int(2));
    assert_eq!(actual.rows[1][2], sibling_wire);
}

async fn assert_bytea_array_grid_edit(connection: &dyn Connection) {
    const ARRAY: &str = r#"{"\\x00ff","\\x5c5c","",NULL}"#;
    const ORACLE: &str = "ARRAY[decode('00ff','hex'),decode('5c5c','hex'),decode('','hex'),NULL]::bytea[]";

    connection
        .execute("CREATE TABLE bytea_array_grid_edit (id integer PRIMARY KEY, value bytea[])")
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO bytea_array_grid_edit VALUES (1, ARRAY[decode('01','hex')]), (2, ARRAY[decode('1234','hex')])",
        )
        .await
        .unwrap();
    let mut rows = connection
        .query("SELECT * FROM bytea_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(value), 'hex') FROM bytea_array_grid_edit WHERE id = 2")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let id_index = rows.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = rows.columns.iter().position(|column| column.name == "value").unwrap();
    rows.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "bytea_array_grid_edit",
        &rows.columns,
        &[(value_index, Value::Text(ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query(&format!("SELECT encode(array_send({ORACLE}), 'hex')"))
        .await
        .unwrap();
    let actual = connection
        .query("SELECT id, encode(array_send(value), 'hex') FROM bytea_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    assert_eq!(actual.rows[0][1], expected.rows[0][0]);
    assert_eq!(actual.rows[1][0], Value::Int(2));
    assert_eq!(actual.rows[1][1], sibling_wire);
}

async fn assert_uuid_array_grid_edit(connection: &dyn Connection) {
    const ARRAY: &str = "{550e8400-e29b-41d4-a716-446655440000,6ba7b810-9dad-11d1-80b4-00c04fd430c8,NULL}";
    const ORACLE: &str =
        "ARRAY['550e8400-e29b-41d4-a716-446655440000'::uuid,'6ba7b810-9dad-11d1-80b4-00c04fd430c8'::uuid,NULL]::uuid[]";

    connection
        .execute("CREATE TABLE uuid_array_grid_edit (id integer PRIMARY KEY, value uuid[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO uuid_array_grid_edit VALUES (1, ARRAY['00000000-0000-0000-0000-000000000001'::uuid]), (2, ARRAY['00000000-0000-0000-0000-000000000002'::uuid])")
        .await
        .unwrap();
    let mut rows = connection
        .query("SELECT * FROM uuid_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(value), 'hex') FROM uuid_array_grid_edit WHERE id = 2")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let id_index = rows.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = rows.columns.iter().position(|column| column.name == "value").unwrap();
    rows.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "uuid_array_grid_edit",
        &rows.columns,
        &[(value_index, Value::Text(ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query(&format!("SELECT encode(array_send({ORACLE}), 'hex')"))
        .await
        .unwrap();
    let actual = connection
        .query("SELECT id, encode(array_send(value), 'hex') FROM uuid_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    assert_eq!(actual.rows[0][1], expected.rows[0][0]);
    assert_eq!(actual.rows[1][0], Value::Int(2));
    assert_eq!(actual.rows[1][1], sibling_wire);
}

async fn assert_timestamptz_array_grid_edit(connection: &dyn Connection) {
    const ARRAY: &str = r#"{"2026-09-30 12:34:56.123456+05:30","1999-12-31 23:59:59.000001-07:00",NULL}"#;
    const ORACLE: &str = "ARRAY['2026-09-30 12:34:56.123456+05:30'::timestamptz,'1999-12-31 23:59:59.000001-07:00'::timestamptz,NULL]::timestamptz[]";

    connection
        .execute("CREATE TABLE timestamptz_array_grid_edit (id integer PRIMARY KEY, value timestamptz[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO timestamptz_array_grid_edit VALUES (1, ARRAY['2000-01-01 00:00:00+00'::timestamptz]), (2, ARRAY['2001-01-01 00:00:00+00'::timestamptz])")
        .await
        .unwrap();
    let mut rows = connection
        .query("SELECT * FROM timestamptz_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let sibling_wire = connection
        .query("SELECT encode(array_send(value), 'hex') FROM timestamptz_array_grid_edit WHERE id = 2")
        .await
        .unwrap()
        .rows[0][0]
        .clone();
    let id_index = rows.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = rows.columns.iter().position(|column| column.name == "value").unwrap();
    rows.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "timestamptz_array_grid_edit",
        &rows.columns,
        &[(value_index, Value::Text(ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query(&format!("SELECT encode(array_send({ORACLE}), 'hex')"))
        .await
        .unwrap();
    let actual = connection
        .query("SELECT id, encode(array_send(value), 'hex') FROM timestamptz_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    assert_eq!(actual.rows[0][1], expected.rows[0][0]);
    assert_eq!(actual.rows[1][0], Value::Int(2));
    assert_eq!(actual.rows[1][1], sibling_wire);
}

pub async fn assert_float8_array_grid_edit(connection: &dyn Connection) {
    const ARRAY: &str = "{1.0000000000000002,-0,5e-324,NaN,Infinity,-Infinity,NULL}";
    const ORACLE: &str = "ARRAY[1.0000000000000002::float8, '-0'::float8, '5e-324'::float8, \
        'NaN'::float8, 'Infinity'::float8, '-Infinity'::float8, NULL::float8]";

    connection
        .execute("CREATE TABLE float8_array_grid_edit (id integer PRIMARY KEY, value float8[])")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO float8_array_grid_edit VALUES (1, ARRAY[0]::float8[]), (2, ARRAY[77]::float8[]), (3, ARRAY[88]::float8[])")
        .await
        .unwrap();
    let mut row = connection
        .query("SELECT * FROM float8_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    let id_index = row.columns.iter().position(|column| column.name == "id").unwrap();
    let value_index = row.columns.iter().position(|column| column.name == "value").unwrap();
    row.columns[id_index].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "float8_array_grid_edit",
        &row.columns,
        &[(value_index, Value::Text(ARRAY.into()))],
        &[Value::Int(1)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let expected = connection
        .query(&format!(
            "SELECT ({ORACLE})::text, encode(array_send(({ORACLE})::float8[]), 'hex')"
        ))
        .await
        .unwrap();
    let actual = connection
        .query(
            "SELECT id, value, value::text, encode(array_send(value), 'hex') \
             FROM float8_array_grid_edit ORDER BY id",
        )
        .await
        .unwrap();
    assert_eq!(actual.rows[0][0], Value::Int(1));
    let Value::Text(native_text) = &expected.rows[0][0] else {
        panic!("native float8[] text oracle must be text: {:?}", expected.rows[0][0]);
    };
    let Value::Text(decoded_text) = &actual.rows[0][1] else {
        panic!("float8[] result must remain exact text: {:?}", actual.rows[0][1]);
    };
    assert_eq!(&actual.rows[0][2..], expected.rows[0].as_slice());
    assert_eq!(
        actual.rows[1][0],
        Value::Int(2),
        "the sibling row identity remains intact"
    );
    assert!(matches!(actual.rows[1][1], Value::Text(_)));

    assert_float8_array_csv_round_trip(connection, &actual, native_text, decoded_text).await;
}

async fn assert_float8_array_csv_round_trip(
    connection: &dyn Connection,
    result: &QueryResult,
    native_text: &str,
    decoded_text: &str,
) {
    let columns = vec![result.columns[0].clone(), result.columns[1].clone()];
    let row = vec![result.rows[0][0].clone(), result.rows[0][1].clone()];
    let csv_text = tablepro_core::export::render_csv(
        &columns,
        std::slice::from_ref(&row),
        &tablepro_core::export::CsvOptions::default(),
    );
    let mut reader = csv::Reader::from_reader(csv_text.as_bytes());
    assert_eq!(reader.headers().unwrap().iter().collect::<Vec<_>>(), ["id", "value"]);
    let exported = reader.records().next().unwrap().unwrap();
    assert_eq!(
        &exported[1], decoded_text,
        "CSV must preserve the driver's exact float8[] value text"
    );
    assert!(reader.records().next().is_none());

    let options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(csv_text.as_bytes(), &options, None).unwrap();
    let imported =
        tablepro_core::import::row_to_values(&sheet.rows[0], &[Some(0), Some(1)], &columns, &options, 2).unwrap();
    assert_eq!(imported, row, "CSV import must retain the array as exact text");

    connection
        .execute("CREATE TABLE float8_array_csv_import (id integer PRIMARY KEY, value float8[])")
        .await
        .unwrap();
    let import_columns = connection.fetch_columns(None, "float8_array_csv_import").await.unwrap();
    let import_mapping = [Some(0), Some(1)];
    let mut import_row = row.clone();
    import_row[0] = Value::Int(4);
    let import_csv = tablepro_core::export::render_csv(
        &columns,
        std::slice::from_ref(&import_row),
        &tablepro_core::export::CsvOptions::default(),
    );
    let import_sheet = tablepro_core::import::read_csv(import_csv.as_bytes(), &options, None).unwrap();
    let insert_plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "postgres",
            schema: None,
            table: "float8_array_csv_import",
            columns: &import_columns,
            mapping: &import_mapping,
        },
        &import_sheet,
        &options,
    )
    .expect("float8[] CSV should build an import plan");
    connection
        .execute_params(&insert_plan.statement, &insert_plan.rows[0])
        .await
        .expect("typed float8[] CSV import must cast array text to the native column type");
    let csv_restored = connection
        .query("SELECT id, value::text, encode(array_send(value), 'hex') FROM float8_array_csv_import")
        .await
        .unwrap();
    assert_eq!(
        csv_restored.rows,
        vec![vec![
            Value::Int(4),
            Value::Text(native_text.into()),
            result.rows[0][3].clone(),
        ]],
        "CSV import must preserve PostgreSQL array text and native wire bytes"
    );

    let mut update_columns = columns;
    update_columns[0].primary_key = true;
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "float8_array_grid_edit",
        &update_columns,
        &[(1, imported[1].clone())],
        &[Value::Int(2)],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);
    let restored = connection
        .query("SELECT id, value::text, encode(array_send(value), 'hex') FROM float8_array_grid_edit ORDER BY id")
        .await
        .unwrap();
    assert_eq!(restored.rows[0][0], Value::Int(1));
    assert_eq!(restored.rows[1][0], Value::Int(2));
    assert_eq!(restored.rows[1][1], Value::Text(native_text.into()));
    assert_eq!(restored.rows[1][2], restored.rows[0][2]);
    assert_eq!(restored.rows[2][0], Value::Int(3));
    assert_eq!(restored.rows[2][2], result.rows[2][3]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_float8_array_grid_edit_preserves_special_and_adjacent_values() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    assert_float8_array_grid_edit(connection.as_ref()).await;
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
