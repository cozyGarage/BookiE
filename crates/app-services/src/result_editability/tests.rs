use bookie_core::sql_syntax::SqlGrammar;
use bookie_core::{ColumnInfo, TableInfo};

use super::{
    ResultLock, ResultRefusal, SourceItem, SourceName, SourceRefusal, SourceTable, editable_target, source_table,
};

fn column(name: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: "text".into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn source(sql: &str) -> Option<SourceTable> {
    source_table(sql, SqlGrammar::PostgreSql).ok()
}

fn named(name: &str) -> SourceItem {
    SourceItem::Column(SourceName {
        name: name.to_owned(),
        quoted: false,
    })
}

#[test]
fn an_aliased_result_edit_targets_the_source_column() {
    use crate::change_tracker::{RowKey, TabChangeTracker};
    use bookie_core::Value;

    let found = source("SELECT id AS row_id, b AS a FROM t").expect("one table");
    let catalog = [column("id", true), column("a", false), column("b", false)];
    let result = [column("row_id", true), column("a", false)];
    let target = editable_target(&found, &result, &catalog).expect("editable");
    let columns = super::mutation_columns(&result, &target);
    let mut tracker = TabChangeTracker::default();
    let key = RowKey::from_pk_values(&[Value::Int(1)]).expect("key");
    tracker.track_cell_edit(key, 1, Value::Text("before".into()), Value::Text("after".into()));
    let (statements, _) = tracker.materialize("postgres", None, "t", &columns).expect("SQL");
    assert!(statements[0].0.contains("SET \"b\" ="), "{}", statements[0].0);
    assert!(statements[0].0.contains("WHERE \"id\" ="), "{}", statements[0].0);
}

#[tokio::test]
async fn saving_an_aliased_result_changes_only_the_source_column() {
    use crate::change_tracker::{RowKey, TabChangeTracker};
    use bookie_core::{ConnectOptions, DatabaseDriver, Value};

    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("aliases.db");
    std::fs::File::create(&path).expect("database file");
    let connection = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: path.to_string_lossy().into_owned(),
            ..Default::default()
        })
        .await
        .expect("connect");
    connection
        .execute("CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b TEXT)")
        .await
        .expect("table");
    connection
        .execute("INSERT INTO t VALUES (1, 'untouched', 'before')")
        .await
        .expect("row");
    let found = source_table("SELECT id AS row_id, b AS a FROM t", SqlGrammar::Sqlite).expect("source");
    let catalog = [column("id", true), column("a", false), column("b", false)];
    let result = connection
        .query("SELECT id AS row_id, b AS a FROM t")
        .await
        .expect("query");
    let target = editable_target(&found, &result.columns, &catalog).expect("editable");
    let columns = super::mutation_columns(&result.columns, &target);
    let mut tracker = TabChangeTracker::default();
    let key = RowKey::from_pk_values(&[Value::Int(1)]).expect("key");
    tracker.track_cell_edit(key, 1, Value::Text("before".into()), Value::Text("after".into()));
    let (statements, _) = tracker.materialize("sqlite", None, "t", &columns).expect("SQL");
    for (sql, params) in statements {
        connection.execute_params(&sql, &params).await.expect("save");
    }
    let saved = connection.query("SELECT id, a, b FROM t").await.expect("saved row");
    assert_eq!(
        saved.rows,
        vec![vec![
            Value::Int(1),
            Value::Text("untouched".into()),
            Value::Text("after".into())
        ]]
    );
}

#[test]
fn aliased_keys_keep_source_names_and_display_labels_separate() {
    let found = source("SELECT id AS row_id, b AS label FROM t").expect("one table");
    let catalog = [column("id", true), column("b", false)];
    let result = [column("row_id", false), column("label", false)];
    let target = editable_target(&found, &result, &catalog).expect("editable");
    let display = super::owned_result_columns(&result, &target);
    let writes = super::mutation_columns(&result, &target);
    assert_eq!(display[0].name, "row_id");
    assert_eq!(display[1].name, "label");
    assert_eq!(writes[0].name, "id");
    assert_eq!(writes[1].name, "b");
    assert!(writes[0].primary_key);
}

#[test]
fn result_staging_refuses_owned_keys_generated_values_and_invalid_nulls() {
    use bookie_core::Value;

    let value = Value::Text("changed".into());
    let mut field = column("value", false);
    assert!(super::can_stage_result_value(&field, &value));
    assert!(super::can_stage_result_value(&field, &Value::Null));
    field.nullable = false;
    assert!(!super::can_stage_result_value(&field, &Value::Null));
    assert!(super::can_stage_result_value(&field, &value));
    field.is_generated = true;
    assert!(!super::can_stage_result_value(&field, &value));
    field.is_generated = false;
    field.is_auto_increment = true;
    assert!(!super::can_stage_result_value(&field, &value));
    assert!(!super::can_stage_result_value(&column("id", true), &value));
}

#[test]
fn expressions_and_duplicate_projections_are_read_only() {
    let found = source("SELECT id, b, b AS duplicate, b || 'x' AS calculated FROM t").expect("one table");
    let catalog = [column("id", true), column("b", false)];
    let result = [
        column("id", true),
        column("b", false),
        column("duplicate", false),
        column("calculated", false),
    ];
    let target = editable_target(&found, &result, &catalog).expect("editable");
    for columns in [
        super::owned_result_columns(&result, &target),
        super::mutation_columns(&result, &target),
    ] {
        assert!(!columns[1].is_generated);
        assert!(columns[2].is_generated);
        assert!(columns[3].is_generated);
        assert!(!columns[2].primary_key);
        assert!(!columns[3].primary_key);
    }
}

#[test]
fn a_select_from_one_table_with_its_key_is_editable() {
    let found = source("SELECT id, name FROM people WHERE id > 1").expect("one table");
    assert_eq!(
        found.table,
        TableInfo {
            schema: None,
            name: "people".into(),
        }
    );
    assert_eq!(found.items, vec![named("id"), named("name")]);
    let catalog = [column("id", true), column("name", false)];
    let result = catalog.clone();
    let target = editable_target(&found, &result, &catalog).expect("editable");
    assert_eq!(target.positions, vec![Some(0), Some(1)]);
}

#[test]
fn a_star_select_is_editable_when_the_catalog_width_matches() {
    let found = source("SELECT * FROM people").expect("one table");
    let catalog = [column("id", true), column("name", false), column("note", false)];
    let result = catalog.clone();
    let target = editable_target(&found, &result, &catalog).expect("editable");
    assert_eq!(target.positions, vec![Some(0), Some(1), Some(2)]);
}

#[test]
fn a_partial_key_stays_read_only() {
    let found = source("SELECT name FROM people").expect("one table");
    let catalog = [column("id", true), column("name", false)];
    let result = [column("name", false)];
    assert_eq!(editable_target(&found, &result, &catalog), Err(ResultLock::KeyNotShown));
}

#[test]
fn a_join_is_refused() {
    assert_eq!(
        source_table("SELECT * FROM a JOIN b ON a.id = b.id", SqlGrammar::PostgreSql),
        Err(SourceRefusal::MoreThanOneTable)
    );
}

#[test]
fn a_grouped_select_is_refused() {
    assert_eq!(
        source_table("SELECT name, count(*) FROM a GROUP BY name", SqlGrammar::PostgreSql),
        Err(SourceRefusal::Collapsed)
    );
}

#[test]
fn every_refusal_variant_is_distinct() {
    use std::collections::HashSet;
    let all = [
        ResultRefusal::NotOneSelect,
        ResultRefusal::MoreThanOneTable,
        ResultRefusal::Collapsed,
        ResultRefusal::Untraced,
        ResultRefusal::NoPrimaryKey,
        ResultRefusal::KeyNotShown,
        ResultRefusal::ColumnsChanged,
        ResultRefusal::ReadOnlySession,
        ResultRefusal::OpenTransaction,
        ResultRefusal::OtherDatabase,
        ResultRefusal::OtherResult,
        ResultRefusal::Checking,
        ResultRefusal::ColumnsUnread,
        ResultRefusal::MultiResult,
    ];
    let ids: HashSet<_> = all.iter().copied().collect();
    assert_eq!(ids.len(), all.len());
}

#[test]
fn a_table_without_a_primary_key_is_refused() {
    let found = source("SELECT id, name FROM people").expect("one table");
    let catalog = [column("id", false), column("name", false)];
    assert_eq!(
        editable_target(&found, &catalog, &catalog),
        Err(ResultLock::NoPrimaryKey)
    );
}
