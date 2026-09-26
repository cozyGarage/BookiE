#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{Connection, OperationControl, Value};
use tokio_util::sync::CancellationToken;

pub async fn assert_time_contract(connection: &dyn Connection) {
    let control = OperationControl::new(CancellationToken::new(), None);
    let mut session = connection.open_session().await.unwrap();
    for zone in ["UTC", "Asia/Kathmandu", "America/New_York"] {
        session
            .query_params_controlled(&format!("SET TIME ZONE '{zone}'"), &[], &control)
            .await
            .unwrap();
        for (kind, input) in [
            ("time", "00:00:00"),
            ("time", "12:34:56.123456"),
            ("time", "23:59:59.999999"),
            ("time", "24:00:00"),
            ("timetz", "00:00:00+00"),
            ("timetz", "12:34:56.123456+05:45"),
            ("timetz", "12:34:56.000001-03:30"),
            ("timetz", "01:02:03+05:45:12"),
            ("timetz", "01:02:03-05:45:12"),
            ("timetz", "24:00:00+15:59:59"),
            ("timetz", "24:00:00-15:59:59"),
        ] {
            let sql =
                format!("SELECT '{input}'::{kind} AS value, encode({kind}_send('{input}'::{kind}), 'hex') AS wire");
            let result = session.query_params_controlled(&sql, &[], &control).await.unwrap();
            assert_eq!(
                result.rows,
                connection.query(&sql).await.unwrap().rows,
                "{zone}: {input}"
            );
            assert_eq!(result.columns[0].data_type, kind.to_ascii_uppercase());
            let value = &result.rows[0][0];
            assert!(
                matches!(value, Value::Time(_) | Value::Text(_)),
                "{zone}: {input}: {value:?}"
            );
            let literal = tablepro_core::sql_literal::render_sql_literal("postgres", value).unwrap();
            let sql = format!("SELECT encode({kind}_send({literal}::{kind}), 'hex')");
            let exported = session.query_params_controlled(&sql, &[], &control).await.unwrap();
            assert_eq!(exported.rows[0][0], result.rows[0][1], "{zone}: {input}: export");
            assert_insert_export(session.as_mut(), &control, kind, &result).await;
            let cast = if matches!(value, Value::Text(_)) { "::text" } else { "" };
            let sql = format!("SELECT encode({kind}_send($1{cast}::{kind}), 'hex')");
            let bound = session
                .query_params_controlled(&sql, std::slice::from_ref(value), &control)
                .await
                .unwrap();
            assert_eq!(bound.rows[0][0], result.rows[0][1], "{zone}: {input}: bound");
            let json = tablepro_core::export::row_to_json(&result.columns, &result.rows[0]);
            let text = match value {
                Value::Time(time) => time.to_string(),
                Value::Text(text) => text.clone(),
                other => panic!("unexpected time: {other:?}"),
            };
            assert_eq!(json["value"].as_str(), Some(text.as_str()));
        }
        let nulls = session
            .query_params_controlled("SELECT NULL::time, NULL::timetz", &[], &control)
            .await
            .unwrap();
        assert_eq!(nulls.rows[0], vec![Value::Null, Value::Null]);
    }
}

async fn assert_insert_export(
    session: &mut dyn tablepro_core::Session,
    control: &OperationControl,
    kind: &str,
    result: &tablepro_core::QueryResult,
) {
    for sql in [
        "DROP TABLE IF EXISTS time_export_target".to_string(),
        format!("CREATE TEMP TABLE time_export_target (value {kind})"),
    ] {
        session.query_params_controlled(&sql, &[], control).await.unwrap();
    }
    let insert = tablepro_core::sql_literal::build_insert_literal(
        "postgres",
        None,
        "time_export_target",
        &result.columns[..1],
        &result.rows[0][..1],
    )
    .unwrap();
    session.query_params_controlled(&insert, &[], control).await.unwrap();
    let sql = format!("SELECT encode({kind}_send(value), 'hex') FROM time_export_target");
    let inserted = session.query_params_controlled(&sql, &[], control).await.unwrap();
    assert_eq!(inserted.rows[0][0], result.rows[0][1], "{kind}: INSERT export");
}
