#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{Connection, OperationControl, QueryResult, Session, Value};
use tokio_util::sync::CancellationToken;

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
            assert_exports(session.as_mut(), &control, kind, &result).await;
        }
    }
}

async fn assert_exports(session: &mut dyn Session, control: &OperationControl, kind: &str, result: &QueryResult) {
    let value = &result.rows[0][0];
    let literal = tablepro_core::sql_literal::render_sql_literal("postgres", value).unwrap();
    let sql = format!("SELECT encode({kind}_send({literal}::{kind}), 'hex')");
    let imported = session
        .query_params_controlled(&sql, &[], control)
        .await
        .unwrap_or_else(|error| panic!("{literal}: {error}"));
    assert_eq!(imported.rows[0][0], result.rows[0][1], "{kind}: literal {literal}");
    let sql = format!("SELECT encode({kind}_send($1::{kind}), 'hex')");
    let bound = session
        .query_params_controlled(&sql, std::slice::from_ref(value), control)
        .await
        .unwrap();
    assert_eq!(bound.rows[0][0], result.rows[0][1], "{kind}: bound {literal}");
    for sql in [
        "DROP TABLE IF EXISTS date_export_target".to_string(),
        format!("CREATE TEMP TABLE date_export_target (value {kind})"),
    ] {
        session.query_params_controlled(&sql, &[], control).await.unwrap();
    }
    let insert = tablepro_core::sql_literal::build_insert_literal(
        "postgres",
        None,
        "date_export_target",
        &result.columns[..1],
        &result.rows[0][..1],
    )
    .unwrap();
    session.query_params_controlled(&insert, &[], control).await.unwrap();
    let sql = format!("SELECT encode({kind}_send(value), 'hex') FROM date_export_target");
    let inserted = session.query_params_controlled(&sql, &[], control).await.unwrap();
    assert_eq!(inserted.rows[0][0], result.rows[0][1], "{kind}: INSERT {literal}");
}
