use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn service_for_policy(path: &std::path::Path) -> DatabaseService {
    let audit_dir = tempfile::tempdir().unwrap();
    DatabaseService::with_loaded_policy(
        AuditRuntime::open_path(audit_dir.path().join("audit.jsonl")),
        load_policy_file(path),
        Some(audit_dir),
    )
}

#[test]
fn an_absent_policy_directory_uses_defaults_but_a_broken_parent_link_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("config");
    let path = parent.join("nested/policy.toml");
    assert!(load_policy_file(&path).is_ok());
    symlink(directory.path().join("missing"), &parent).unwrap();
    assert!(load_policy_file(&path).is_err());
}

#[test]
fn a_readable_policy_link_preserves_its_restrictions() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.toml");
    let path = directory.path().join("policy.toml");
    std::fs::write(&target, "[environments.local]\nhuman_approve_ddl = true\n").unwrap();
    symlink(target, &path).unwrap();
    let policy = load_policy_file(&path).unwrap();
    assert!(policy.for_environment(Environment::Local).human_approve_ddl);
}

#[test]
fn a_permission_denied_policy_path_is_not_treated_as_absent() {
    let directory = tempfile::tempdir().unwrap();
    let locked = directory.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("policy.toml"), "").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let enforced = std::fs::read_dir(&locked).is_err();
    let result = load_policy_file(&locked.join("policy.toml"));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!enforced || result.is_err());
}

#[tokio::test]
async fn a_policy_load_failure_refuses_bootstrap_and_active_handles_until_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("policy.toml");
    symlink(directory.path().join("target.toml"), &path).unwrap();
    let service = service_for_policy(&path);
    let id = open_memory_connection(&service, "blocked").await;
    assert!(!service.policy_available());
    assert!(service.get(id).is_none());
    let raw = service
        .connections
        .lock()
        .unwrap()
        .get(&id)
        .unwrap()
        .inner
        .lock()
        .unwrap()
        .connection
        .clone();
    assert!(
        service
            .candidate_guard_factory()
            .guard(raw, &service.metadata(id).unwrap(), Principal::human_gui())
            .is_err()
    );
    std::fs::write(directory.path().join("target.toml"), "").unwrap();
    service.reload_policy_file(&path).unwrap();
    assert!(service.policy_available());
    assert!(service.get(id).unwrap().query("SELECT 1").await.is_ok());
    service.close(id);
}

#[tokio::test]
async fn a_failed_reload_keeps_the_last_loaded_restrictions_and_a_valid_reload_recovers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("policy.toml");
    std::fs::write(&path, "[environments.local]\nhuman_approve_ddl = true\n").unwrap();
    let service = service_for_policy(&path);
    let id = open_memory_connection(&service, "restricted").await;
    std::fs::write(&path, "[environments.nowhere]\n").unwrap();
    assert!(service.reload_policy_file(&path).is_err());
    assert!(service.policy_available());
    let guard = service.get(id).unwrap();
    assert!(matches!(
        guard.execute("CREATE TABLE restricted (id INTEGER)").await,
        Err(tablepro_core::DriverError::PolicyDenied(_))
    ));
    std::fs::write(&path, "").unwrap();
    service.reload_policy_file(&path).unwrap();
    assert!(
        service
            .get(id)
            .unwrap()
            .execute("CREATE TABLE recovered (id INTEGER)")
            .await
            .is_ok()
    );
    service.close(id);
}
