#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{Connection, DriverError, OperationControl, Value};
use tokio_util::sync::CancellationToken;

fn assert_undecodable_temporal_exports(result: &tablepro_core::QueryResult, type_name: &str) {
    let value = result.rows[0][0].clone();
    let rows = vec![vec![value]];
    let columns = &result.columns[..1];
    let json = tablepro_core::export::render_json(columns, &rows);
    let parsed_json: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed_json,
        serde_json::json!([{"value": format!("<undecodable {type_name}>")}])
    );

    let options = tablepro_core::export::CsvOptions::default();
    let csv = tablepro_core::export::render_csv(columns, &rows, &options);
    let parsed_csv = tablepro_core::import::read_csv(csv.as_bytes(), &Default::default(), None).unwrap();
    assert_eq!(parsed_csv.rows[0][0], format!("<undecodable {type_name}>"));
}

pub async fn assert_date_contract(connection: &dyn Connection) {
    let control = OperationControl::new(CancellationToken::new(), None);
    let mut session = connection.open_session().await.unwrap();
    for zone in ["UTC", "Asia/Kathmandu", "America/New_York"] {
        session
            .query_params_controlled(&format!("SET TIME ZONE '{zone}'"), &[], &control)
            .await
            .unwrap();
        for (kind, input) in [
            ("date", "0001-01-01 BC"),
            ("date", "0002-12-31 BC"),
            ("date", "4713-01-01 BC"),
            ("date", "0001-01-01"),
            ("date", "9999-12-31"),
            ("date", "10000-01-01"),
            ("date", "262000-02-29"),
            ("timestamp", "0001-01-01 12:34:56.123456 BC"),
            ("timestamp", "0002-12-31 23:59:59.999999 BC"),
            ("timestamp", "10000-01-01 00:00:00.000001"),
            ("timestamp", "1999-12-31 23:59:59.999999"),
            ("timestamptz", "0001-01-01 12:34:56.123456+00 BC"),
            ("timestamptz", "10000-01-01 00:00:00.000001+00"),
            ("timestamptz", "2024-11-03 01:30:00-04"),
            ("timestamptz", "2024-11-03 01:30:00-05"),
        ] {
            let sql =
                format!("SELECT '{input}'::{kind} AS value, encode({kind}_send('{input}'::{kind}), 'hex') AS wire");
            let result = session.query_params_controlled(&sql, &[], &control).await.unwrap();
            assert_eq!(
                result.rows,
                connection.query(&sql).await.unwrap().rows,
                "{zone}: {input}"
            );
            assert!(
                matches!(
                    result.rows[0][0],
                    Value::Date(_) | Value::DateTime(_) | Value::TimestampTz(_)
                ),
                "{input}: {:?}",
                result.rows
            );
            assert_eq!(result.columns[0].data_type, kind.to_ascii_uppercase());
            crate::wire_round_trip::assert_wire_round_trip(
                session.as_mut(),
                &control,
                kind,
                &result.columns[0],
                &result.rows[0][0],
                &result.rows[0][1],
            )
            .await;
        }
    }

    let sql = "SELECT '1000000-01-01'::date AS value, '1000000-01-01'::date::text AS server_text, encode(date_send('1000000-01-01'::date), 'hex') AS server_wire";
    let result = session.query_params_controlled(sql, &[], &control).await.unwrap();
    assert_eq!(result.rows, connection.query(sql).await.unwrap().rows);
    assert_eq!(result.rows[0][0], Value::Undecodable("DATE".into()));
    assert_eq!(result.rows[0][1], Value::Text("1000000-01-01".into()));
    assert!(matches!(&result.rows[0][2], Value::Text(wire) if !wire.is_empty()));
    assert_undecodable_temporal_exports(&result, "DATE");
    assert_eq!(
        tablepro_core::sql_literal::render_sql_literal("postgres", &result.rows[0][0]),
        Err(tablepro_core::sql_literal::LiteralError::Undecodable)
    );
    assert!(matches!(
        connection
            .query_params("SELECT $1::date", std::slice::from_ref(&result.rows[0][0]))
            .await,
        Err(DriverError::Unsupported(_))
    ));

    let sql = "SELECT TIMESTAMP '294276-12-31 23:59:59.999999' AS value, TIMESTAMP '294276-12-31 23:59:59.999999'::text AS server_text, encode(timestamp_send(TIMESTAMP '294276-12-31 23:59:59.999999'), 'hex') AS server_wire";
    let result = session.query_params_controlled(sql, &[], &control).await.unwrap();
    assert_eq!(result.rows, connection.query(sql).await.unwrap().rows);
    assert_eq!(result.rows[0][0], Value::Undecodable("TIMESTAMP".into()));
    assert_eq!(result.rows[0][1], Value::Text("294276-12-31 23:59:59.999999".into()));
    assert!(matches!(&result.rows[0][2], Value::Text(wire) if !wire.is_empty()));
    assert_undecodable_temporal_exports(&result, "TIMESTAMP");
    assert_eq!(
        tablepro_core::sql_literal::render_sql_literal("postgres", &result.rows[0][0]),
        Err(tablepro_core::sql_literal::LiteralError::Undecodable)
    );
    assert!(matches!(
        connection
            .query_params("SELECT $1::timestamp", std::slice::from_ref(&result.rows[0][0]))
            .await,
        Err(DriverError::Unsupported(_))
    ));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_bc_dates_survive_default_csv_import() {
    let (_container, options) = crate::start_pg().await;
    let connection = crate::connect(options).await;
    let options = tablepro_core::import::CsvImportOptions::default();
    for (kind, send_fn, bc, ad, last, beyond) in [
        (
            "date",
            "date_send",
            "0001-12-31 BC",
            "0001-01-01",
            "9999-12-31",
            "10000-01-01",
        ),
        (
            "timestamp",
            "timestamp_send",
            "0001-12-31 23:59:59.999999 BC",
            "0001-01-01 00:00:00.000001",
            "9999-12-31 23:59:59.999999",
            "10000-01-01 00:00:00.000001",
        ),
        (
            "timestamptz",
            "timestamptz_send",
            "0001-12-31 23:59:59.999999+00 BC",
            "0001-01-01 00:00:00.000001+00",
            "9999-12-31 23:59:59.999999+00",
            "10000-01-01 00:00:00.000001+00",
        ),
    ] {
        let source_table = format!("era_csv_source_{kind}");
        let target_table = format!("era_csv_target_{kind}");
        connection
            .execute(&format!(
                "CREATE TABLE {source_table} (id integer PRIMARY KEY, value {kind})"
            ))
            .await
            .unwrap();
        connection
            .execute(&format!(
                "INSERT INTO {source_table} VALUES (1, '{bc}'::{kind}), (2, '{ad}'::{kind}), (3, '{last}'::{kind}), (4, '{beyond}'::{kind}), (5, NULL)"
            ))
            .await
            .unwrap();
        let source = connection
            .query(&format!(
                "SELECT id, value, encode({send_fn}(value), 'hex') AS wire FROM {source_table} ORDER BY id"
            ))
            .await
            .unwrap();
        let csv_columns = &source.columns[..2];
        let csv_rows: Vec<_> = source.rows.iter().map(|row| row[..2].to_vec()).collect();
        let csv =
            tablepro_core::export::render_csv(csv_columns, &csv_rows, &tablepro_core::export::CsvOptions::default());
        let sheet = tablepro_core::import::read_csv(csv.as_bytes(), &options, None).unwrap();

        connection
            .execute(&format!(
                "CREATE TABLE {target_table} (id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY, value {kind})"
            ))
            .await
            .unwrap();
        let columns = connection.fetch_columns(None, &target_table).await.unwrap();
        let mapping = [None, Some(1)];
        let plan = tablepro_core::import::build_insert_plan(
            &tablepro_core::import::ImportTarget {
                driver_id: "postgres",
                schema: None,
                table: &target_table,
                columns: &columns,
                mapping: &mapping,
            },
            &sheet,
            &options,
        )
        .unwrap();
        assert!(
            matches!(&plan.rows[0][0], Value::Text(text) if text.ends_with(" BC")),
            "{kind}: {:?}",
            plan.rows
        );
        assert_eq!(plan.rows[4], vec![Value::Null]);
        for row in &plan.rows {
            connection.execute_params(&plan.statement, row).await.unwrap();
        }
        let restored = connection
            .query(&format!(
                "SELECT value::text, encode({send_fn}(value), 'hex') AS wire FROM {target_table} ORDER BY id"
            ))
            .await
            .unwrap();
        assert_eq!(
            restored.rows.iter().map(|row| &row[1]).collect::<Vec<_>>(),
            source.rows.iter().map(|row| &row[2]).collect::<Vec<_>>(),
            "{kind} native bytes changed through CSV"
        );
        assert!(
            matches!(&restored.rows[0][0], Value::Text(text) if text.ends_with(" BC")),
            "{kind}: {:?}",
            restored.rows
        );
    }
}
