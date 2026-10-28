use super::*;

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
fn a_dangling_policy_link_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::os::unix::fs::symlink(dir.path().join("missing.toml"), &path).unwrap();
    assert!(load_policy_file(&path).is_err());
}

#[test]
fn a_dangling_config_directory_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("configured");
    std::os::unix::fs::symlink(dir.path().join("missing"), &parent).unwrap();
    assert!(load_policy_file(&parent.join("policy.toml")).is_err());
}

#[test]
fn permission_denied_policy_targets_are_errors_when_permissions_are_enforced() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.toml");
    let path = dir.path().join("policy.toml");
    std::fs::write(&target, "").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();
    let enforced = std::fs::read_to_string(&target).is_err();
    let result = load_policy_file(&path);
    let available = service_from_policy_file(&path).policy_available();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    if !enforced {
        eprintln!("permission-denied coverage unavailable: this process can read a mode-000 target");
        return;
    }
    assert!(result.is_err());
    assert!(!available);
}

#[tokio::test]
async fn unavailable_policy_refuses_local_and_agent_handles() {
    let service = DatabaseService::new_isolated();
    let id = open_memory_connection(&service, "policy test").await;
    *service.policy.lock().unwrap() = None;
    assert!(service.handle(id, Principal::human_gui()).is_none());
    assert!(
        service
            .handle(
                id,
                Principal::Agent {
                    token: "test".into(),
                    client: None,
                    model: None
                }
            )
            .is_none()
    );
}

fn service_from_policy_file(path: &std::path::Path) -> DatabaseService {
    let dir = tempfile::tempdir().unwrap();
    let audit = AuditRuntime::open_path(dir.path().join("audit.jsonl"));
    DatabaseService::with_audit_policy(audit, load_policy_file(path), Some(dir))
}

#[tokio::test]
async fn malformed_startup_policy_refuses_candidates_and_recovers_after_valid_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::write(&path, "[environments.nowhere]\n").unwrap();
    let service = service_from_policy_file(&path);
    let id = open_memory_connection(&service, "policy recovery").await;
    assert!(!service.policy_available());
    assert!(service.get(id).is_none());
    let connection: Arc<dyn Connection> = Arc::from(
        drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: ":memory:".into(),
                ..Default::default()
            })
            .await
            .unwrap(),
    );
    let metadata = ConnectionMetadata {
        id,
        name: "candidate".into(),
        driver_id: "sqlite".into(),
        environment: Environment::Local,
        read_only: false,
        server_version: None,
        query_timeout_secs: None,
    };
    assert!(matches!(
        service
            .candidate_guard_factory()
            .guard(connection.clone(), &metadata, Principal::human_gui()),
        Err(tablepro_core::DriverError::PolicyDenied(_))
    ));
    std::fs::write(
        &path,
        "[environments.local]\nhuman_approve_writes = true\nhuman_approve_ddl = true\n",
    )
    .unwrap();
    service.apply_policy_reload(load_policy_file(&path)).unwrap();
    assert!(service.policy_available());
    assert!(service.get(id).is_some());
    let guard = service
        .candidate_guard_factory()
        .guard(connection, &metadata, Principal::human_gui())
        .unwrap();
    assert!(matches!(
        guard.execute("CREATE TABLE restricted (id INTEGER)").await,
        Err(tablepro_core::DriverError::PolicyDenied(_))
    ));
    std::fs::write(&path, "[environments.nowhere]\n").unwrap();
    assert!(service.apply_policy_reload(load_policy_file(&path)).is_err());
    assert!(service.policy_available());
    let guarded = service.get(id).unwrap();
    assert!(matches!(
        guarded.execute("INSERT INTO restricted VALUES (1)").await,
        Err(tablepro_core::DriverError::PolicyDenied(_))
    ));
}

#[test]
fn optional_absence_and_valid_policy_are_available_but_a_broken_link_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    assert!(service_from_policy_file(&path).policy_available());
    std::fs::write(
        &path,
        "[environments.local]\nhuman_approve_writes = true\nhuman_approve_ddl = true\n",
    )
    .unwrap();
    assert!(service_from_policy_file(&path).policy_available());
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(dir.path().join("missing.toml"), &path).unwrap();
    assert!(!service_from_policy_file(&path).policy_available());
}

#[test]
fn a_policy_directory_and_non_directory_parent_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.toml");
    std::fs::create_dir(&path).unwrap();
    assert!(load_policy_file(&path).is_err());
    assert!(!service_from_policy_file(&path).policy_available());
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, "").unwrap();
    assert!(load_policy_file(&path.join("policy.toml")).is_err());
}
