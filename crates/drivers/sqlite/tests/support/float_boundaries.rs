#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[tokio::test]
async fn sqlite_real_storage_preserves_float_edge_bits() {
    let connection = SqliteDriver.connect(options_for(":memory:")).await.unwrap();
    connection
        .execute("CREATE TABLE float_edges (id INTEGER PRIMARY KEY, value REAL)")
        .await
        .unwrap();
    let values = [f64::from_bits(1), f64::MAX, f64::INFINITY, f64::NEG_INFINITY];
    for (index, value) in values.into_iter().enumerate() {
        connection
            .execute_params(
                "INSERT INTO float_edges VALUES (?1, ?2)",
                &[Value::Int(index as i64 + 1), Value::Float(value)],
            )
            .await
            .unwrap();
    }

    let stored = connection
        .query("SELECT typeof(value), value FROM float_edges ORDER BY id")
        .await
        .unwrap();
    assert_eq!(stored.rows.len(), values.len());
    for (row, expected) in stored.rows.iter().zip(values) {
        assert_eq!(row[0], Value::Text("real".into()));
        let Value::Float(actual) = row[1] else {
            panic!("SQLite REAL storage must decode as a float: {:?}", row[1]);
        };
        assert_eq!(actual.to_bits(), expected.to_bits());
    }

    let literals = ["4.9406564584124654e-324", "1.7976931348623157e308", "9e999", "-9e999"];
    for (index, literal) in literals.iter().enumerate() {
        let matched = connection
            .query(&format!(
                "SELECT value = {literal} FROM float_edges WHERE id = {}",
                index + 1
            ))
            .await
            .unwrap();
        assert_eq!(
            matched.rows,
            [[Value::Int(1)]],
            "SQLite reads {literal} as the stored value"
        );
    }
    connection
        .execute("CREATE TABLE native_float_edges (id INTEGER PRIMARY KEY, value REAL)")
        .await
        .unwrap();
    for (index, literal) in literals.iter().enumerate() {
        connection
            .execute(&format!(
                "INSERT INTO native_float_edges VALUES ({}, {literal})",
                index + 1
            ))
            .await
            .unwrap();
    }
    let native = connection
        .query("SELECT value FROM native_float_edges ORDER BY id")
        .await
        .unwrap();
    for (row, expected) in native.rows.iter().zip(values) {
        let Value::Float(actual) = row[0] else {
            panic!("a natively stored REAL must decode as a float: {:?}", row[0]);
        };
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
}

#[tokio::test]
async fn sqlite_real_binding_refuses_negative_zero_and_nan() {
    let connection = SqliteDriver.connect(options_for(":memory:")).await.unwrap();
    connection
        .execute("CREATE TABLE float_edges (id INTEGER PRIMARY KEY, value REAL)")
        .await
        .unwrap();

    for (id, value, boundary) in [(2, f64::NAN, "NaN"), (1, -0.0, "negative zero")] {
        let error = connection
            .execute_params(
                "INSERT INTO float_edges VALUES (?1, ?2)",
                &[Value::Int(id), Value::Float(value)],
            )
            .await
            .expect_err(boundary);
        assert!(
            matches!(error, tablepro_core::DriverError::Unsupported(_)),
            "{boundary}: {error:?}"
        );
    }

    let columns = connection.fetch_columns(None, "float_edges").await.unwrap();
    let literal_error = tablepro_core::sql_literal::build_insert_literal(
        "sqlite",
        None,
        "float_edges",
        &columns,
        &[Value::Int(3), Value::Float(-0.0)],
    )
    .expect_err("SQLite SQL export must not erase negative zero");
    assert!(matches!(
        literal_error,
        tablepro_core::sql_dialect::BuildSqlError::UnrepresentableValue { column } if column == "value"
    ));

    let after_refusal = connection.query("SELECT count(*) FROM float_edges").await.unwrap();
    assert_eq!(after_refusal.rows, vec![vec![Value::Int(0)]]);
}
