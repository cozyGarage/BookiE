#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_app_services::workspace_state::{ConnectionWorkspaceState, WorkspaceStore, WorkspaceTabRecord};
use tablepro_core::{ColumnInfo, Value};
use uuid::Uuid;

fn state(a: Uuid, b: Uuid, first: &str, second: &str) -> ConnectionWorkspaceState {
    ConnectionWorkspaceState {
        tabs: vec![
            WorkspaceTabRecord::Editor { query: first.into(), draft_id: Some(a), file: None },
            WorkspaceTabRecord::Editor { query: second.into(), draft_id: Some(b), file: None },
        ],
        ..Default::default()
    }
}

fn flush(store: &WorkspaceStore) -> Result<(), tablepro_app_services::workspace_state::WorkspaceFlushError> {
    store.flush().recv_timeout(std::time::Duration::from_secs(20)).unwrap()
}

#[test]
fn a_failed_draft_batch_preserves_the_previous_first_draft() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let drafts = root.path().join("drafts");
    let connection = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let store = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    store.save_connection(connection, state(first, second, "old first", "old second"));
    assert_eq!(flush(&store), Ok(()));
    let metadata = std::fs::read(&path).unwrap();
    let directory = drafts.join(connection.to_string());
    let blocked = directory.join(format!("{second}.sql"));
    std::fs::remove_file(&blocked).unwrap();
    std::fs::create_dir(&blocked).unwrap();
    store.save_connection(connection, state(first, second, "new first", "new second"));
    assert!(flush(&store).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), metadata);
    assert_eq!(std::fs::read_to_string(directory.join(format!("{first}.sql"))).unwrap(), "old first");
}

#[test]
fn an_unreadable_unrequested_connection_does_not_block_the_requested_workspace() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("workspace.json");
    let drafts = root.path().join("drafts");
    let broken = Uuid::new_v4();
    let healthy = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let writer = WorkspaceStore::new(Some(path.clone()), drafts.clone());
    writer.save_connection(broken, state(first, second, "broken draft", "other"));
    writer.save_connection(healthy, ConnectionWorkspaceState::default());
    assert_eq!(flush(&writer), Ok(()));
    std::fs::write(drafts.join(broken.to_string()).join(format!("{first}.sql")), [0xff]).unwrap();
    let reader = WorkspaceStore::new(Some(path), drafts);
    assert_eq!(reader.prefetch_connections(&[healthy]), Ok(()));
    assert!(reader.load_connection(healthy).is_some());
}

#[test]
fn a_case_only_concurrent_change_is_not_overwritten() {
    let column = |name: &str, primary_key| ColumnInfo {
        name: name.into(), data_type: "text".into(), nullable: false, primary_key,
        is_auto_increment: false, default_value: None, is_generated: false, comment: None,
        collation: None, enum_type: None, domain_type: None,
    };
    let (sql, _) = tablepro_core::sql_dialect::build_optimistic_keyed_update(
        "sqlite", None, "people", &[column("id", true), column("name", false)],
        &[(1, Value::Text("Ada".into()), Value::Text("stale edit".into()))], &[Value::Int(1)],
    ).unwrap();
    let program = r#"import sqlite3,sys
c=sqlite3.connect(':memory:')
c.execute('CREATE TABLE people(id INTEGER PRIMARY KEY, name TEXT COLLATE NOCASE)')
c.execute("INSERT INTO people VALUES(1,'Ada')")
c.execute("UPDATE people SET name='ADA' WHERE id=1")
n=c.execute(sys.argv[1], ('stale edit',1,'Ada')).rowcount
print(n)
print(c.execute('SELECT name FROM people').fetchone()[0])
"#;
    let output = std::process::Command::new("python3").args(["-c", program, &sql]).output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0\nADA\n", "SQL: {sql}");
}
