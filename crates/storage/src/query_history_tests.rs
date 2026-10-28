use super::*;

async fn legacy_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("memory pool");
    for statement in BASE_SCHEMA {
        sqlx::query(*statement).execute(&pool).await.expect("legacy schema");
    }
    sqlx::query(
        "INSERT INTO history (query, driver_id, connection_id, connection_name, executed_at, success, cancelled) \
             VALUES ('SELECT 1', 'sqlite', '00000000-0000-0000-0000-000000000000', 'legacy', 1, 1, 0)",
    )
    .execute(&pool)
    .await
    .expect("legacy row");
    pool
}

fn store_over(pool: SqlitePool) -> HistoryStore {
    HistoryStore {
        pool,
        path: PathBuf::from(":memory:"),
    }
}

#[tokio::test]
async fn a_database_written_before_versioning_is_stamped_with_the_current_version() {
    let pool = legacy_pool().await;
    assert_eq!(user_version(&pool).await.unwrap(), 0);

    apply_schema(&pool).await.expect("upgrade");

    assert_eq!(user_version(&pool).await.unwrap(), MIGRATIONS.len() as i64);
}

#[tokio::test]
async fn upgrading_an_older_database_keeps_the_entries_it_already_holds() {
    let pool = legacy_pool().await;
    apply_schema(&pool).await.expect("upgrade");

    let entries = store_over(pool).search(SearchFilter::default()).await.unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].query, "SELECT 1");
}

#[tokio::test]
async fn upgrading_a_history_file_keeps_a_private_pre_migration_snapshot() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.db");
    let options = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    for statement in BASE_SCHEMA {
        sqlx::query(*statement).execute(&pool).await.unwrap();
    }
    sqlx::query("PRAGMA user_version = 1").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO history (query, driver_id, connection_id, connection_name, executed_at, success) VALUES ('SELECT 1', 'sqlite', '', '', 0, 1)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let store = HistoryStore::open(path.clone()).await.unwrap();
    let backup = path.with_extension("before-migrations.db");
    assert_eq!(std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
    let backup_options = SqliteConnectOptions::new().filename(&backup);
    let old = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(backup_options)
        .await
        .unwrap();
    assert_eq!(user_version(&old).await.unwrap(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM history")
            .fetch_one(&old)
            .await
            .unwrap(),
        1
    );
    assert_eq!(user_version(&store.pool).await.unwrap(), MIGRATIONS.len() as i64);
    let before = std::fs::read(&backup).unwrap();
    old.close().await;
    store.pool.close().await;
    let reopened = HistoryStore::open(path.clone()).await.unwrap();
    assert_eq!(std::fs::read(&backup).unwrap(), before);
    reopened.pool.close().await;
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
    }
    std::fs::copy(&backup, &path).unwrap();
    let restored = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    assert_eq!(user_version(&restored).await.unwrap(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM history")
            .fetch_one(&restored)
            .await
            .unwrap(),
        1
    );
    restored.close().await;
}

#[tokio::test]
async fn reopening_an_up_to_date_database_applies_nothing_further() {
    let pool = legacy_pool().await;
    apply_schema(&pool).await.expect("upgrade");
    apply_schema(&pool).await.expect("second open");

    assert_eq!(user_version(&pool).await.unwrap(), MIGRATIONS.len() as i64);
    assert!(pending_migrations(user_version(&pool).await.unwrap()).is_empty());
}

#[tokio::test]
async fn a_failed_history_migration_rolls_back_its_schema_and_version_change() {
    let pool = legacy_pool().await;
    sqlx::query("ALTER TABLE history ADD COLUMN source TEXT NOT NULL DEFAULT 'editor'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA user_version = 1").execute(&pool).await.unwrap();

    assert!(apply_schema(&pool).await.is_err());

    assert_eq!(user_version(&pool).await.unwrap(), 1);
    let columns = sqlx::query("PRAGMA table_info(history)")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        columns
            .iter()
            .filter(|row| row.get::<String, _>("name") == "source")
            .count(),
        1
    );
    let query: String = sqlx::query_scalar("SELECT query FROM history WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(query, "SELECT 1");
}

#[tokio::test]
async fn an_older_database_gains_the_source_column_and_reads_as_editor() {
    let pool = legacy_pool().await;

    apply_schema(&pool).await.expect("upgrade");

    let entries = store_over(pool).search(SearchFilter::default()).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, Source::Editor);
}

#[tokio::test]
async fn opening_a_legacy_history_file_migrates_it_and_reopening_keeps_its_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let options = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
    let legacy = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("legacy file");
    for statement in BASE_SCHEMA {
        sqlx::query(*statement).execute(&legacy).await.expect("legacy schema");
    }
    sqlx::query(
        "INSERT INTO history (query, driver_id, connection_id, connection_name, executed_at, success, cancelled) \
             VALUES ('SELECT 1', 'sqlite', '00000000-0000-0000-0000-000000000000', 'legacy', 1, 1, 0)",
    )
    .execute(&legacy)
    .await
    .expect("legacy row");
    legacy.close().await;

    let store = HistoryStore::open(path.clone())
        .await
        .expect("migrate legacy history file");
    let entries = store.search(SearchFilter::default()).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].query, "SELECT 1");
    assert_eq!(entries[0].source, Source::Editor);
    assert_eq!(user_version(&store.pool).await.unwrap(), MIGRATIONS.len() as i64);

    // A previous package can keep using its explicit legacy column list
    // and omit `source`; SQLite supplies the migration's editor default.
    sqlx::query(
        "INSERT INTO history (query, driver_id, connection_id, connection_name, executed_at, success, cancelled) \
             VALUES ('SELECT 2', 'sqlite', '00000000-0000-0000-0000-000000000000', 'legacy', 2, 1, 0)",
    )
    .execute(&store.pool)
    .await
    .expect("legacy package write after migration");
    let legacy_read: (i64, String) = sqlx::query_as("SELECT id, query FROM history ORDER BY executed_at DESC LIMIT 1")
        .fetch_one(&store.pool)
        .await
        .expect("legacy package explicit-column read");
    assert_eq!(legacy_read.1, "SELECT 2");
    drop(store);

    let reopened = HistoryStore::open(path).await.expect("reopen migrated history file");
    let entries = reopened.search(SearchFilter::default()).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .any(|entry| entry.query == "SELECT 1" && entry.source == Source::Editor)
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.query == "SELECT 2" && entry.source == Source::Editor)
    );
    assert_eq!(user_version(&reopened.pool).await.unwrap(), MIGRATIONS.len() as i64);
}

#[tokio::test]
async fn full_text_search_still_finds_an_entry_written_before_the_upgrade() {
    let pool = legacy_pool().await;
    apply_schema(&pool).await.expect("upgrade");

    let found = store_over(pool)
        .search(SearchFilter {
            needle: Some("SELECT".into()),
            ..SearchFilter::default()
        })
        .await
        .unwrap();

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn a_source_filter_keeps_only_the_entries_that_came_from_it() {
    let store = fresh_store().await;
    for source in [Source::Editor, Source::Structure, Source::Browse] {
        store
            .record(NewEntry {
                source,
                ..sample_new_entry()
            })
            .await
            .unwrap();
    }

    let structure = store
        .search(SearchFilter {
            source: Some(Source::Structure),
            ..SearchFilter::default()
        })
        .await
        .unwrap();

    assert_eq!(structure.len(), 1);
    assert_eq!(structure[0].source, Source::Structure);
    assert_eq!(store.search(SearchFilter::default()).await.unwrap().len(), 3);
}

#[tokio::test]
async fn statements_that_carry_credentials_never_reach_the_history_file() {
    let store = fresh_store().await;
    for (driver_id, query) in [
        ("postgres", "ALTER ROLE app PASSWORD 'secret'"),
        ("mysql", "CREATE USER 'u'@'%' IDENTIFIED BY 'secret'"),
        ("postgres", "SELECT dblink_connect('host=h password=secret')"),
    ] {
        store
            .record(NewEntry {
                query: query.into(),
                driver_id: driver_id.into(),
                ..sample_new_entry()
            })
            .await
            .unwrap();
    }
    store.record(sample_new_entry()).await.unwrap();

    let entries = store.search(SearchFilter::default()).await.unwrap();
    assert_eq!(entries.len(), 1, "only the plain statement is kept");
    assert_eq!(entries[0].query, "SELECT 1");
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM history WHERE query LIKE '%secret%'")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(stored, 0);
}

#[tokio::test]
async fn an_unknown_stored_source_reads_as_editor_rather_than_failing() {
    let store = fresh_store().await;
    store.record(sample_new_entry()).await.unwrap();
    sqlx::query("UPDATE history SET source = 'from-the-future'")
        .execute(&store.pool)
        .await
        .unwrap();

    let entries = store.search(SearchFilter::default()).await.unwrap();

    assert_eq!(entries[0].source, Source::Editor);
}

#[tokio::test]
async fn a_version_from_a_newer_build_is_left_alone() {
    let pool = legacy_pool().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "PRAGMA user_version = {}",
        MIGRATIONS.len() + 5
    )))
    .execute(&pool)
    .await
    .unwrap();

    apply_schema(&pool).await.expect("no downgrade");

    assert_eq!(user_version(&pool).await.unwrap(), MIGRATIONS.len() as i64 + 5);
}

async fn fresh_store() -> HistoryStore {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("memory pool");
    apply_schema(&pool).await.expect("apply schema");
    HistoryStore {
        pool,
        path: PathBuf::from(":memory:"),
    }
}

#[tokio::test]
async fn rejects_query_over_limit() {
    let store = fresh_store().await;
    let big = "x".repeat(MAX_QUERY_BYTES + 1);
    let entry = NewEntry {
        query: big,
        driver_id: "sqlite".into(),
        connection_id: Uuid::nil(),
        connection_name: "test".into(),
        executed_at: SystemTime::now(),
        duration_ms: Some(1),
        rows_affected: Some(0),
        outcome: Outcome::Success,
        source: Source::Editor,
    };
    let err = store.record(entry).await.unwrap_err();
    assert!(matches!(err, StorageError::TooLarge { .. }));
}

#[tokio::test]
async fn stores_keep_independent_pools() {
    let first = fresh_store().await;
    let second = fresh_store().await;
    first
        .record(NewEntry {
            query: "SELECT 1".into(),
            driver_id: "sqlite".into(),
            connection_id: Uuid::nil(),
            connection_name: "first".into(),
            executed_at: SystemTime::now(),
            duration_ms: None,
            rows_affected: None,
            outcome: Outcome::Success,
            source: Source::Editor,
        })
        .await
        .unwrap();

    assert_eq!(first.search(SearchFilter::default()).await.unwrap().len(), 1);
    assert!(second.search(SearchFilter::default()).await.unwrap().is_empty());
}

#[test]
fn csv_escaping() {
    assert_eq!(csv_field("plain"), "plain");
    assert_eq!(csv_field("a,b"), "\"a,b\"");
    assert_eq!(csv_field("a\"b"), "\"a\"\"b\"");
    assert_eq!(csv_field("line\n"), "\"line\n\"");
}

fn sample_new_entry() -> NewEntry {
    NewEntry {
        query: "SELECT 1".into(),
        driver_id: "sqlite".into(),
        connection_id: Uuid::nil(),
        connection_name: "test".into(),
        executed_at: SystemTime::now(),
        duration_ms: None,
        rows_affected: None,
        outcome: Outcome::Success,
        source: Source::Editor,
    }
}

fn sample_entry() -> Entry {
    Entry {
        id: 1,
        query: "SELECT 1".into(),
        driver_id: "sqlite".into(),
        connection_id: Uuid::nil(),
        connection_name: "test".into(),
        executed_at: SystemTime::now(),
        duration_ms: None,
        rows_affected: None,
        success: true,
        cancelled: false,
        pinned: false,
        error: None,
        source: Source::Editor,
    }
}

#[test]
fn exported_sql_keeps_untrusted_text_inside_comments() {
    let mut entry = sample_entry();
    entry.connection_name = "prod\nDROP TABLE users;\r\n-- ".into();
    entry.driver_id = "sqlite\r\u{2028}DROP TABLE a;".into();
    entry.error = Some("boom\rDROP TABLE b;\nDROP TABLE c;\u{85}DROP TABLE d;".into());

    let exported = sql_export_entry(&entry);

    for line in exported.split(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}']) {
        assert!(
            line.is_empty() || line.starts_with("-- ") || line == "SELECT 1;",
            "a line escaped its comment: {line:?}"
        );
    }
    assert!(exported.contains("prod"));
    assert!(exported.ends_with("SELECT 1;\n\n"));
}

#[test]
fn outcome_summary_reports_cancelled_before_checking_success() {
    let mut entry = sample_entry();
    entry.cancelled = true;
    entry.success = false;
    assert_eq!(outcome_summary(&entry), "cancelled");
}

#[test]
fn outcome_summary_reports_an_error_outcome() {
    let mut entry = sample_entry();
    entry.success = false;
    assert_eq!(outcome_summary(&entry), "error");
}

#[test]
fn outcome_summary_reports_rows_and_duration_when_both_are_known() {
    let mut entry = sample_entry();
    entry.rows_affected = Some(3);
    entry.duration_ms = Some(12);
    assert_eq!(outcome_summary(&entry), "ok · 3 row(s) · 12 ms");
}

#[test]
fn outcome_summary_reports_rows_only_when_duration_is_unknown() {
    let mut entry = sample_entry();
    entry.rows_affected = Some(3);
    assert_eq!(outcome_summary(&entry), "ok · 3 row(s)");
}

#[test]
fn outcome_summary_reports_duration_only_when_rows_are_unknown() {
    let mut entry = sample_entry();
    entry.duration_ms = Some(12);
    assert_eq!(outcome_summary(&entry), "ok · 12 ms");
}

#[test]
fn outcome_summary_reports_plain_ok_when_neither_is_known() {
    let entry = sample_entry();
    assert_eq!(outcome_summary(&entry), "ok");
}

#[test]
fn a_fresh_history_db_is_created_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    create_private_file_if_missing(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn restrict_permissions_tightens_the_database_and_its_wal_and_shm_siblings() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let wal = dir.path().join("history.db-wal");
    let shm = dir.path().join("history.db-shm");
    for p in [&path, &wal, &shm] {
        std::fs::write(p, b"").unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    restrict_permissions(&path).unwrap();

    for p in [&path, &wal, &shm] {
        let mode = std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{p:?} must be private");
    }
}

#[tokio::test]
async fn a_history_file_from_a_newer_version_is_refused_and_left_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.db");
    let store = HistoryStore::open(path.clone()).await.unwrap();
    store.pool.close().await;
    let newer = MIGRATIONS.len() as i64 + 1;
    let options = SqliteConnectOptions::new().filename(&path);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("PRAGMA user_version = {newer}")))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let error = HistoryStore::open(path.clone())
        .await
        .err()
        .expect("a newer file is refused");

    assert!(
        matches!(&error, StorageError::Schema(message) if message.contains(&newer.to_string())),
        "{error:?}"
    );
    let options = SqliteConnectOptions::new().filename(&path);
    let check = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    assert_eq!(user_version(&check).await.unwrap(), newer);
    assert!(!path.with_extension("before-migrations.db").exists());
}
