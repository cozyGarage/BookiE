use super::*;
use tablepro_core::DatabaseDriver;

#[path = "database_service/tests/identity_tests.rs"]
mod identity_tests;

#[test]
fn policy_file_is_read_from_the_build_profile_config_directory() {
    let path = xdg_config_path("policy.toml").unwrap();
    let expected = std::path::Path::new(crate::config::storage_dir_name()).join("policy.toml");
    assert!(path.ends_with(expected));
}

#[test]
fn a_missing_policy_file_uses_defaults_and_an_invalid_one_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    assert!(load_policy_file(&path).is_ok());
    std::fs::write(&path, "[environments.nowhere]\n").unwrap();
    assert!(load_policy_file(&path).is_err());
}

#[test]
fn separate_instances_do_not_share_connections() {
    let a = DatabaseService::new();
    let b = DatabaseService::new();
    let id = Uuid::new_v4();
    assert!(!a.is_active(id));
    assert!(!b.is_active(id));
}

#[test]
fn unavailable_audit_runtime_disables_governed_writes() {
    let audit = AuditRuntime::unavailable();

    assert!(!audit.available);
    assert!(audit.state.governed_writes_disabled());
}

#[tokio::test]
async fn recovered_unknown_write_blocks_only_its_saved_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.jsonl");
    let connection_id = Uuid::new_v4();
    let journal = AuditJournal::open_validated(path.clone()).unwrap();
    let mut event = tablepro_storage::sample_event(Principal::human_gui());
    event.connection_id = connection_id;
    event.phase = AuditRecordPhase::Intent;
    event.operation_class = tablepro_policy::AuditOperationClass::Mutation;
    event.terminal_status = AuditTerminalStatus::Pending;
    journal.append_durable(event).await.unwrap();
    drop(journal);

    let runtime = AuditRuntime::open_path(path);
    let service = DatabaseService::with_audit(runtime, Some(dir));
    let other_connection_id = Uuid::new_v4();
    assert!(!service.audit_state.governed_writes_disabled());
    assert!(service.governed_writes_disabled(connection_id));
    assert!(!service.governed_writes_disabled(other_connection_id));
    let factory = service.candidate_guard_factory();
    assert!(factory.connection_audit_state(connection_id).governed_writes_disabled());
    assert!(
        !factory
            .connection_audit_state(other_connection_id)
            .governed_writes_disabled()
    );
}

async fn open_memory_connection(service: &DatabaseService, name: &str) -> Uuid {
    let driver: Arc<dyn DatabaseDriver> = Arc::new(drivers_sqlite::SqliteDriver);
    let id = Uuid::new_v4();
    let options = ConnectOptions {
        database: ":memory:".into(),
        ..Default::default()
    };
    let connection = driver.connect(options.clone()).await.expect("sqlite connection");
    let activated = service.activate(
        id,
        ConnectionMetadata {
            id,
            name: name.into(),
            driver_id: "sqlite".into(),
            environment: Environment::Local,
            read_only: false,
            server_version: None,
            query_timeout_secs: None,
        },
        connection,
        None,
        false,
        ReconnectParams {
            driver,
            opts: options,
            ssh: None,
            environment: tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
        },
    );
    assert!(activated, "the id is freshly generated, so activation must succeed");
    id
}

async fn assert_read_only_write_is_refused(
    connection: &dyn tablepro_core::Connection,
    sql: &str,
    params: Vec<tablepro_core::Value>,
    expect_one: &[usize],
    control: &tablepro_core::OperationControl,
) {
    let statements = [(sql.to_string(), params)];
    let result = connection
        .execute_in_transaction_checked_controlled(&statements, expect_one, control)
        .await;

    assert!(
        matches!(result, Err(tablepro_core::DriverError::PolicyDenied(_))),
        "{sql}"
    );
}

fn assert_three_denied_mutations_are_audited(service: &DatabaseService, connection_id: Uuid) {
    let journal = service._audit_temp_dir.as_ref().unwrap().path().join("audit.jsonl");
    let contents = std::fs::read_to_string(journal).expect("audit journal exists");
    let events: Vec<serde_json::Value> = contents
        .lines()
        .map(|line| serde_json::from_str(line).expect("journal record is valid JSON"))
        .collect();
    let denied_mutations = events
        .iter()
        .filter(|record| {
            let event = &record["event"];
            event["connection_id"] == connection_id.to_string()
                && event["operation_class"] == "mutation"
                && event["phase"] == "outcome"
                && event["terminal_status"] == "denied"
        })
        .count();

    assert_eq!(denied_mutations, 3, "each refused write needs a terminal audit outcome");
}

#[tokio::test]
async fn gui_read_only_guarded_transactions_deny_insert_update_and_delete() {
    let service = DatabaseService::new_isolated();
    let driver: Arc<dyn DatabaseDriver> = Arc::new(drivers_sqlite::SqliteDriver);
    let id = Uuid::new_v4();
    let options = ConnectOptions {
        database: ":memory:".into(),
        ..Default::default()
    };
    let connection = driver.connect(options.clone()).await.expect("sqlite connection");
    let setup_control = tablepro_core::OperationControl::with_timeout(std::time::Duration::from_secs(5));
    connection
        .execute_controlled(
            "CREATE TABLE guarded_notes (id INTEGER PRIMARY KEY, value TEXT NOT NULL)",
            &setup_control,
        )
        .await
        .expect("create fixture table");
    connection
        .execute_controlled("INSERT INTO guarded_notes VALUES (1, 'before')", &setup_control)
        .await
        .expect("seed fixture row");
    assert!(service.activate(
        id,
        ConnectionMetadata {
            id,
            name: "read-only fixture".into(),
            driver_id: "sqlite".into(),
            environment: Environment::Local,
            read_only: true,
            server_version: None,
            query_timeout_secs: None,
        },
        connection,
        None,
        true,
        ReconnectParams {
            driver,
            opts: options,
            ssh: None,
            environment: tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
        },
    ));

    let guarded = service.get(id).expect("active GUI connection");
    let control = tablepro_core::OperationControl::with_timeout(std::time::Duration::from_secs(5));
    for (sql, params, expect_one) in [
        (
            "INSERT INTO guarded_notes VALUES (?, ?)",
            vec![
                tablepro_core::Value::Int(2),
                tablepro_core::Value::Text("inserted".into()),
            ],
            vec![],
        ),
        (
            "UPDATE guarded_notes SET value = ? WHERE id = ?",
            vec![
                tablepro_core::Value::Text("updated".into()),
                tablepro_core::Value::Int(1),
            ],
            vec![0],
        ),
        ("DELETE FROM guarded_notes WHERE id = 1", vec![], vec![0]),
    ] {
        assert_read_only_write_is_refused(guarded.as_ref(), sql, params, &expect_one, &control).await;
    }
    let rows = guarded
        .query("SELECT value FROM guarded_notes WHERE id = 1")
        .await
        .unwrap()
        .rows;
    assert_eq!(rows, [[tablepro_core::Value::Text("before".into())]]);
    assert_three_denied_mutations_are_audited(&service, id);
    service.close(id);
}

#[tokio::test]
async fn gui_local_write_records_successful_terminal_audit_outcome() {
    let service = DatabaseService::new_isolated();
    let id = open_memory_connection(&service, "local write fixture").await;
    let guarded = service.get(id).expect("active GUI connection");

    guarded
        .execute("CREATE TABLE guarded_notes (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .await
        .expect("local schema setup");
    guarded
        .execute("INSERT INTO guarded_notes VALUES (1, 'saved')")
        .await
        .expect("local GUI write");

    let journal = service._audit_temp_dir.as_ref().unwrap().path().join("audit.jsonl");
    let contents = std::fs::read_to_string(journal).expect("audit journal exists");
    let records: Vec<serde_json::Value> = contents
        .lines()
        .map(|line| serde_json::from_str(line).expect("journal record is valid JSON"))
        .collect();
    let write_events: Vec<_> = records
        .iter()
        .filter(|record| {
            let event = &record["event"];
            event["connection_id"] == id.to_string() && event["operation_class"] == "mutation"
        })
        .collect();

    assert_eq!(write_events.len(), 2, "the insert retains intent and outcome records");
    assert!(write_events.iter().any(|record| record["event"]["phase"] == "intent"));
    assert!(write_events.iter().any(|record| {
        let event = &record["event"];
        event["phase"] == "outcome" && event["terminal_status"] == "succeeded"
    }));
    let rows = guarded
        .query("SELECT value FROM guarded_notes WHERE id = 1")
        .await
        .unwrap()
        .rows;
    assert_eq!(rows, [[tablepro_core::Value::Text("saved".into())]]);
    service.close(id);
}

#[tokio::test]
async fn candidate_catalog_reads_use_the_durable_policy_audit_journal() {
    let service = DatabaseService::new_isolated();
    let driver: Arc<dyn DatabaseDriver> = Arc::new(drivers_sqlite::SqliteDriver);
    let connection: Arc<dyn Connection> = Arc::from(
        driver
            .connect(ConnectOptions {
                database: ":memory:".into(),
                ..Default::default()
            })
            .await
            .expect("sqlite connection"),
    );
    let metadata = ConnectionMetadata {
        id: Uuid::new_v4(),
        name: "candidate".into(),
        driver_id: "sqlite".into(),
        environment: Environment::Prod,
        read_only: true,
        server_version: None,
        query_timeout_secs: None,
    };
    let guard = service
        .candidate_guard_factory()
        .guard(connection, &metadata, Principal::human_gui());

    guard.list_tables().await.expect("guarded table listing");
    guard.list_views().await.expect("guarded view listing");

    let journal = service._audit_temp_dir.as_ref().unwrap().path().join("audit.jsonl");
    let lines = std::fs::read_to_string(journal).expect("audit journal exists");
    let events: Vec<serde_json::Value> = lines.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    let metadata_events: Vec<_> = events
        .iter()
        .filter(|record| record["event"]["decision_rule"] == "metadata_read")
        .collect();
    assert_eq!(
        metadata_events.len(),
        4,
        "two metadata reads each have intent and outcome"
    );
    assert!(metadata_events.iter().all(|record| {
        record["event"]["connection_id"] == metadata.id.to_string() && record["event"]["connection_name"] == "candidate"
    }));
    let successful_outcomes = metadata_events
        .iter()
        .filter(|record| record["event"]["phase"] == "outcome" && record["event"]["terminal_status"] == "succeeded")
        .count();
    assert_eq!(successful_outcomes, 2);
}

#[tokio::test]
async fn administrative_bundle_audit_writes_sanitized_intent_and_outcome_pair() {
    let service = DatabaseService::new_isolated();
    let operation_id = Uuid::new_v4();

    service
        .audit_administrative_action(
            operation_id,
            AuditAdministrativeAction::ConnectionBundleImport,
            AuditRecordPhase::Intent,
            AuditTerminalStatus::Pending,
            3,
            None,
        )
        .await
        .expect("durable administrative intent");
    service
        .audit_administrative_action(
            operation_id,
            AuditAdministrativeAction::ConnectionBundleImport,
            AuditRecordPhase::Outcome,
            AuditTerminalStatus::Succeeded,
            3,
            Some(12),
        )
        .await
        .expect("durable administrative outcome");

    let journal = service._audit_temp_dir.as_ref().unwrap().path().join("audit.jsonl");
    let contents = std::fs::read_to_string(journal).expect("audit journal exists");
    let records: Vec<serde_json::Value> = contents
        .lines()
        .map(|line| serde_json::from_str(line).expect("journal record is valid JSON"))
        .collect();
    assert_eq!(records.len(), 2);
    for (record, phase) in records.iter().zip(["intent", "outcome"]) {
        let event = &record["event"];
        assert_eq!(event["operation_id"], operation_id.to_string());
        assert_eq!(event["phase"], phase);
        assert_eq!(event["operation_class"], "administrative");
        assert_eq!(event["connection_id"], Uuid::nil().to_string());
        assert_eq!(event["connection_name"], "local_settings");
        assert_eq!(event["driver_id"], "bookie");
        assert_eq!(event["administrative_action"]["action"], "connection_bundle_import");
        assert_eq!(event["administrative_action"]["affected_count"], 3);
        assert!(event.get("transport_attempt").is_none_or(serde_json::Value::is_null));
        let encoded = event.to_string();
        for forbidden in ["/tmp/private-bundle", "connection-secret", "private-key-material"] {
            assert!(!encoded.contains(forbidden));
        }
    }
    assert_eq!(records[1]["event"]["terminal_status"], "succeeded");
}

#[tokio::test]
async fn administrative_bundle_intent_fails_closed_when_audit_is_unavailable() {
    let audit_dir = tempfile::tempdir().expect("audit temp dir");
    let service = DatabaseService::with_audit(AuditRuntime::unavailable(), Some(audit_dir));

    let result = service
        .audit_administrative_action(
            Uuid::new_v4(),
            AuditAdministrativeAction::ConnectionBundleExport,
            AuditRecordPhase::Intent,
            AuditTerminalStatus::Pending,
            1,
            None,
        )
        .await;

    assert!(result.is_err());
    assert!(service.audit_state.governed_writes_disabled());
}

#[tokio::test]
async fn candidate_metadata_read_refuses_when_its_audit_intent_cannot_be_written() {
    let audit_dir = tempfile::tempdir().expect("audit temp dir");
    let service = DatabaseService::with_audit(AuditRuntime::unavailable(), Some(audit_dir));
    let driver: Arc<dyn DatabaseDriver> = Arc::new(drivers_sqlite::SqliteDriver);
    let connection: Arc<dyn Connection> = Arc::from(
        driver
            .connect(ConnectOptions {
                database: ":memory:".into(),
                ..Default::default()
            })
            .await
            .expect("sqlite connection"),
    );
    let metadata = ConnectionMetadata {
        id: Uuid::new_v4(),
        name: "candidate".into(),
        driver_id: "sqlite".into(),
        environment: Environment::Prod,
        read_only: true,
        server_version: None,
        query_timeout_secs: None,
    };
    let guard = service
        .candidate_guard_factory()
        .guard(connection, &metadata, Principal::human_gui());

    let result = guard.list_tables().await;
    assert!(matches!(result, Err(tablepro_core::DriverError::PolicyDenied(_))));
}

#[tokio::test]
async fn activation_is_additive_and_keeps_earlier_connections() {
    let service = DatabaseService::new();

    let first = open_memory_connection(&service, "first").await;
    assert_eq!(service.all_connections().len(), 1);

    let second = open_memory_connection(&service, "second").await;
    assert_eq!(service.all_connections().len(), 2);
    assert!(service.handle(first, Principal::human_gui()).is_some());
    assert!(service.handle(second, Principal::human_gui()).is_some());
}

#[tokio::test]
async fn a_disabled_connection_does_not_block_writes_on_its_sibling() {
    let service = DatabaseService::new_isolated();
    let first = open_memory_connection(&service, "first").await;
    let second = open_memory_connection(&service, "second").await;
    let first_inner = service.connections.lock().unwrap().get(&first).unwrap().inner.clone();
    first_inner.lock().unwrap().audit_state = Arc::new(AuditState::with_governed_writes_disabled());

    assert!(service.governed_writes_disabled(first));
    assert!(!service.governed_writes_disabled(second));

    service.close(first);
    service.close(second);
}

/// H3: two windows opening the same saved connection must not let the
/// second one silently steal the first's entry. Before this fix,
/// `connections.insert(id, entry)` always overwrote, so window A's
/// connection handle would silently start answering for window B's
/// session, and A's original entry (and its reconnect-monitor task)
/// leaked with no cancellation.
#[tokio::test]
async fn a_second_window_cannot_steal_an_already_active_connection() {
    let service = DatabaseService::new();
    let id = Uuid::new_v4();
    let driver: Arc<dyn DatabaseDriver> = Arc::new(drivers_sqlite::SqliteDriver);
    let options = ConnectOptions {
        database: ":memory:".into(),
        ..Default::default()
    };
    let first_metadata = ConnectionMetadata {
        id,
        name: "window A".into(),
        driver_id: "sqlite".into(),
        environment: Environment::Local,
        read_only: false,
        server_version: None,
        query_timeout_secs: None,
    };
    let first_conn = driver.connect(options.clone()).await.expect("sqlite connection");
    assert!(service.activate(
        id,
        first_metadata,
        first_conn,
        None,
        false,
        ReconnectParams {
            driver: driver.clone(),
            opts: options.clone(),
            ssh: None,
            environment: tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
        },
    ));

    let second_metadata = ConnectionMetadata {
        id,
        name: "window B".into(),
        driver_id: "sqlite".into(),
        environment: Environment::Local,
        read_only: false,
        server_version: None,
        query_timeout_secs: None,
    };
    let second_conn = driver.connect(options.clone()).await.expect("sqlite connection");
    let activated = service.activate(
        id,
        second_metadata,
        second_conn,
        None,
        false,
        ReconnectParams {
            driver,
            opts: options,
            ssh: None,
            environment: tablepro_transport::SshEnvironment::builtin(tablepro_ssh::UnknownHostKey::Learn),
        },
    );

    assert!(
        !activated,
        "a second window must not overwrite the first window's entry"
    );
    assert_eq!(
        service.metadata(id).map(|m| m.name),
        Some("window A".into()),
        "window A's entry must survive the refused activation untouched"
    );
    assert!(service.is_active(id));
}

#[tokio::test]
async fn closing_one_connection_leaves_every_other_connection_usable() {
    let service = DatabaseService::new();
    let first = open_memory_connection(&service, "first").await;
    let second = open_memory_connection(&service, "second").await;

    service.close(second);

    assert_eq!(service.all_connections().len(), 1);
    assert!(service.handle(second, Principal::human_gui()).is_none());
    let survivor = service.handle(first, Principal::human_gui()).expect("first survives");
    survivor.ping().await.expect("the surviving connection still answers");
}

#[tokio::test]
async fn each_connection_keeps_its_own_metadata() {
    let service = DatabaseService::new();
    let first = open_memory_connection(&service, "first").await;
    let second = open_memory_connection(&service, "second").await;

    assert_eq!(service.metadata(first).map(|m| m.name), Some("first".to_string()));
    assert_eq!(service.metadata(second).map(|m| m.name), Some("second".to_string()));

    service.close(first);

    assert!(service.metadata(first).is_none());
    assert_eq!(service.metadata(second).map(|m| m.name), Some("second".to_string()));
}
