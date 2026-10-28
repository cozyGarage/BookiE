use tablepro_app_services::change_tracker::{RowKey, TabChangeTracker};
use tablepro_core::{Connection, DriverError, Value, sql_dialect::build_keyed_update};

pub async fn assert_conflict(
    connection: &dyn Connection,
    driver: &str,
    old: Value,
    concurrent: Value,
    search_matches: Option<usize>,
) {
    let columns = connection.fetch_columns(None, "exact_grid").await.unwrap();
    for id in [1, 2] {
        let seed = tablepro_core::sql_dialect::build_insert_from_draft(
            driver,
            None,
            "exact_grid",
            &columns,
            &[Value::Int(id), old.clone()],
        )
        .unwrap();
        connection.execute_params(&seed.0, &seed.1).await.unwrap();
    }
    let loaded = connection
        .query("SELECT id, v FROM exact_grid ORDER BY id")
        .await
        .unwrap();
    assert_eq!(loaded.rows[0][1], old);
    let row = RowKey::from_pk_values(&[Value::Int(1)]).unwrap();
    let mut tracker = TabChangeTracker::new();
    let draft = match &old {
        Value::Bytes(_) => Value::Bytes(vec![0x73, 0x61, 0x76, 0x65]),
        Value::JsonText(_) => Value::JsonText("{\"saved\":true}".into()),
        Value::Text(_) if columns[1].data_type.ends_with("[]") => Value::Text("{\"saved\"}".into()),
        _ => Value::Text("saved".into()),
    };
    tracker.track_cell_edit(row.clone(), 1, old.clone(), draft.clone());
    let before = tracker.materialize(driver, None, "exact_grid", &columns).unwrap().0;
    let external = build_keyed_update(
        driver,
        None,
        "exact_grid",
        &columns,
        &[(1, concurrent.clone())],
        &[Value::Int(1)],
    )
    .unwrap();
    connection.execute_params(&external.0, &external.1).await.unwrap();
    let native_before = native_values(connection, driver).await;
    assert_search_behavior(connection, driver, &columns, &old, search_matches).await;
    let result = connection.execute_in_transaction_checked(&before, &[0]).await;
    assert!(
        matches!(&result, Err(DriverError::Transaction { source, .. }) if matches!(source.as_ref(), DriverError::ConcurrentModification)),
        "{driver} {}: {result:?}",
        columns[1].data_type,
    );
    assert_eq!(native_values(connection, driver).await, native_before);
    let after = tracker.materialize(driver, None, "exact_grid", &columns).unwrap().0;
    assert_eq!(after, before);
    assert_eq!(tracker.current_cell_value(&row, 1, &old), &draft);
    assert_eq!(tracker.pending_count(), 1);
    let persisted = connection.query("SELECT v FROM exact_grid ORDER BY id").await.unwrap();
    assert_eq!(persisted.rows, vec![vec![concurrent.clone()], vec![old]]);
    assert_fresh_save(connection, driver, concurrent, draft).await;
}

async fn assert_fresh_save(connection: &dyn Connection, driver: &str, concurrent: Value, draft: Value) {
    let columns = connection.fetch_columns(None, "exact_grid").await.unwrap();
    let row = RowKey::from_pk_values(&[Value::Int(1)]).unwrap();
    let mut fresh = TabChangeTracker::new();
    fresh.track_cell_edit(row, 1, concurrent, draft.clone());
    let statements = fresh.materialize(driver, None, "exact_grid", &columns).unwrap().0;
    assert_eq!(
        connection
            .execute_in_transaction_checked(&statements, &[0])
            .await
            .unwrap(),
        vec![1]
    );
    assert_eq!(
        connection
            .query("SELECT v FROM exact_grid WHERE id = 1")
            .await
            .unwrap()
            .rows,
        vec![vec![draft]]
    );
}

async fn native_values(connection: &dyn Connection, driver: &str) -> Vec<Vec<Value>> {
    let expression = match driver {
        "sqlite" => "hex(v)",
        "mysql" => "HEX(v)",
        "postgres" => "encode(pg_catalog.record_send(ROW(v)), 'hex')",
        "mssql" => "CONVERT(varchar(max), CONVERT(varbinary(max), v), 2)",
        _ => unreachable!(),
    };
    connection
        .query(&format!("SELECT id, {expression} FROM exact_grid ORDER BY id"))
        .await
        .unwrap()
        .rows
}

async fn assert_search_behavior(
    connection: &dyn Connection,
    driver: &str,
    columns: &[tablepro_core::ColumnInfo],
    old: &Value,
    expected: Option<usize>,
) {
    let Some(expected) = expected else {
        return;
    };
    let set = tablepro_core::FilterSet {
        rules: vec![tablepro_core::equality_rule("v", old).unwrap()],
        ..Default::default()
    };
    let (clause, params) = tablepro_core::build_filter_where(driver, columns, &set)
        .unwrap()
        .unwrap();
    let result = connection
        .query_params(
            &format!("SELECT id FROM exact_grid WHERE {clause} ORDER BY id"),
            &params,
        )
        .await
        .unwrap();
    assert_eq!(result.rows.len(), expected);
}

pub async fn assert_key_conflict(connection: &dyn Connection, driver: &str, kind: &str) {
    connection
        .execute(&format!("CREATE TABLE exact_key (id {kind} PRIMARY KEY, v INT)"))
        .await
        .unwrap();
    let columns = connection.fetch_columns(None, "exact_key").await.unwrap();
    for (edit_key, concurrent_key) in [(false, "ADA"), (true, "ADA"), (false, "Ada "), (true, "Ada ")] {
        connection.execute("DELETE FROM exact_key").await.unwrap();
        connection
            .execute("INSERT INTO exact_key VALUES ('Ada', 1), ('Zoe', 2)")
            .await
            .unwrap();
        let original_key = Value::Text("Ada".into());
        let row = RowKey::from_pk_values(std::slice::from_ref(&original_key)).unwrap();
        let mut tracker = TabChangeTracker::new();
        let (index, original, draft) = if edit_key {
            (0, original_key.clone(), Value::Text("New".into()))
        } else {
            (1, Value::Int(1), Value::Int(3))
        };
        tracker.track_cell_edit(row, index, original, draft);
        let statements = tracker.materialize(driver, None, "exact_key", &columns).unwrap().0;
        let external = build_keyed_update(
            driver,
            None,
            "exact_key",
            &columns,
            &[(0, Value::Text(concurrent_key.into()))],
            std::slice::from_ref(&original_key),
        )
        .unwrap();
        connection.execute_params(&external.0, &external.1).await.unwrap();
        let before = connection
            .query("SELECT id, v FROM exact_key ORDER BY v")
            .await
            .unwrap()
            .rows;
        let result = connection.execute_in_transaction_checked(&statements, &[0]).await;
        assert!(
            matches!(result, Err(DriverError::Transaction { source, .. }) if matches!(*source, DriverError::ConcurrentModification))
        );
        assert_eq!(
            connection
                .query("SELECT id, v FROM exact_key ORDER BY v")
                .await
                .unwrap()
                .rows,
            before
        );
        assert_eq!(
            tracker.materialize(driver, None, "exact_key", &columns).unwrap().0,
            statements
        );
        assert_eq!(tracker.pending_count(), 1);
    }
    connection.execute("DROP TABLE exact_key").await.unwrap();
}
