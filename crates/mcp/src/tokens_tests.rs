use std::os::unix::fs::symlink;
use std::process::Command;
use std::time::{Duration, Instant};

use super::*;
use tempfile::TempDir;

#[test]
fn the_token_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    let store = TokenStore::open(path.clone()).unwrap();
    store
        .issue("t".into(), TokenPermissions::ReadOnly, vec![Uuid::new_v4()], None)
        .unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "token file mode was {mode:o}");
}

#[test]
fn an_existing_token_file_with_group_or_other_access_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    std::fs::write(&path, b"[]").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let Err(error) = TokenStore::open(path) else {
        panic!("a permissive store must not open");
    };
    assert!(error.contains("only by its owner"), "{error}");
}

#[test]
fn an_existing_owner_only_token_file_opens() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    std::fs::write(&path, b"[]").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    TokenStore::open(path).unwrap();
}

#[test]
fn a_symlink_token_file_is_rejected() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target.json");
    let path = dir.path().join("tokens.json");
    std::fs::write(&target, b"[]").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(target, &path).unwrap();

    let Err(error) = TokenStore::open(path) else {
        panic!("a symlink store must not open");
    };
    assert!(error.contains("cannot open token store"), "{error}");
}

#[test]
fn a_non_regular_token_file_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    std::fs::create_dir(&path).unwrap();

    let Err(error) = TokenStore::open(path) else {
        panic!("a non-regular store must not open");
    };
    assert!(error.contains("must be a regular file"), "{error}");
}

#[test]
fn an_oversized_token_file_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    std::fs::write(&path, vec![b' '; MAX_TOKEN_STORE_BYTES as usize + 1]).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let Err(error) = TokenStore::open(path) else {
        panic!("an oversized store must not open");
    };
    assert!(error.contains("maximum token store size"), "{error}");
}

#[test]
fn a_corrupt_token_file_is_an_error_rather_than_an_empty_store() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    std::fs::write(&path, b"{ this is not a token list").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let Err(error) = TokenStore::open(path) else {
        panic!("a corrupt store must not open");
    };
    assert!(error.contains("not a readable token store"), "{error}");
}

#[test]
fn issue_and_authenticate() {
    let dir = TempDir::new().unwrap();
    let store = TokenStore::open(dir.path().join("tokens.json")).unwrap();
    let (_meta, plain) = store
        .issue("cursor".into(), TokenPermissions::ReadWrite, vec![], None)
        .unwrap();
    let authed = store.authenticate(&plain).unwrap();
    assert_eq!(authed.name, "cursor");
    assert!(store.authenticate("bogus").is_err());
}

#[test]
fn an_expired_token_is_refused() {
    let dir = TempDir::new().unwrap();
    let store = TokenStore::open(dir.path().join("tokens.json")).unwrap();
    let (_meta, expired) = store
        .issue(
            "old".into(),
            TokenPermissions::ReadOnly,
            vec![],
            Some(Utc::now() - chrono::Duration::seconds(1)),
        )
        .unwrap();
    let (_meta, current) = store
        .issue(
            "new".into(),
            TokenPermissions::ReadOnly,
            vec![],
            Some(Utc::now() + chrono::Duration::hours(1)),
        )
        .unwrap();

    let refused = store.authenticate(&expired).unwrap_err();
    assert!(refused.contains("expired"), "{refused}");
    assert_eq!(store.authenticate(&current).unwrap().name, "new");
}

#[test]
fn the_token_file_and_debug_never_hold_the_plaintext() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    let store = TokenStore::open(path.clone()).unwrap();
    let (meta, plain) = store
        .issue("agent".into(), TokenPermissions::ReadWrite, vec![], None)
        .unwrap();

    assert!(!std::fs::read_to_string(&path).unwrap().contains(&plain));
    assert_ne!(meta.token_hash, plain);
    assert!(!format!("{meta:?}").contains(&plain));
}

#[test]
fn revoke_blocks() {
    let dir = TempDir::new().unwrap();
    let store = TokenStore::open(dir.path().join("tokens.json")).unwrap();
    let (meta, plain) = store
        .issue("x".into(), TokenPermissions::ReadOnly, vec![], None)
        .unwrap();
    store.revoke(meta.id).unwrap();
    assert!(store.authenticate(&plain).is_err());
}

#[test]
fn a_permissive_fixed_temporary_file_is_not_reused() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    let old_temporary_path = path.with_extension("json.tmp");
    std::fs::write(&old_temporary_path, b"untrusted").unwrap();
    std::fs::set_permissions(&old_temporary_path, std::fs::Permissions::from_mode(0o666)).unwrap();
    let store = TokenStore::open(path.clone()).unwrap();

    store
        .issue("safe".into(), TokenPermissions::ReadOnly, vec![], None)
        .unwrap();

    assert_eq!(std::fs::read(&old_temporary_path).unwrap(), b"untrusted");
    let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn a_subprocess_issue_cannot_resurrect_a_revoked_token() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tokens.json");
    let issuer_ready = dir.path().join("issuer-ready");
    let revoke_complete = dir.path().join("revoke-complete");
    let store = TokenStore::open(path.clone()).unwrap();
    let (revoked, revoked_plaintext) = store
        .issue("revoked".into(), TokenPermissions::ReadOnly, vec![], None)
        .unwrap();
    drop(store);
    let executable = std::env::current_exe().unwrap();
    let mut issuer = token_subprocess(&executable, "issue", &path, revoked.id, &issuer_ready, &revoke_complete);
    let mut revoker = token_subprocess(
        &executable,
        "revoke",
        &path,
        revoked.id,
        &issuer_ready,
        &revoke_complete,
    );

    assert!(issuer.wait().unwrap().success());
    assert!(revoker.wait().unwrap().success());

    let reopened = TokenStore::open(path).unwrap();
    assert_eq!(reopened.list().len(), 2);
    assert!(reopened.authenticate(&revoked_plaintext).is_err());
}

fn token_subprocess(
    executable: &Path,
    action: &str,
    path: &Path,
    token_id: Uuid,
    issuer_ready: &Path,
    revoke_complete: &Path,
) -> std::process::Child {
    Command::new(executable)
        .args(["--exact", "tokens::tests::subprocess_mutates_token_store", "--ignored"])
        .env("TABLEPRO_TOKEN_TEST_ACTION", action)
        .env("TABLEPRO_TOKEN_TEST_PATH", path)
        .env("TABLEPRO_TOKEN_TEST_ID", token_id.to_string())
        .env("TABLEPRO_TOKEN_TEST_ISSUER_READY", issuer_ready)
        .env("TABLEPRO_TOKEN_TEST_REVOKE_COMPLETE", revoke_complete)
        .spawn()
        .unwrap()
}

fn wait_for_path(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {}", path.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "helper process spawned by other tests through --exact --ignored, not a standalone test"]
fn subprocess_mutates_token_store() {
    let action = std::env::var("TABLEPRO_TOKEN_TEST_ACTION").unwrap();
    let path = PathBuf::from(std::env::var_os("TABLEPRO_TOKEN_TEST_PATH").unwrap());
    let token_id = std::env::var("TABLEPRO_TOKEN_TEST_ID").unwrap().parse().unwrap();
    let issuer_ready = PathBuf::from(std::env::var_os("TABLEPRO_TOKEN_TEST_ISSUER_READY").unwrap());
    let revoke_complete = PathBuf::from(std::env::var_os("TABLEPRO_TOKEN_TEST_REVOKE_COMPLETE").unwrap());

    if action == "issue" {
        let store = TokenStore::open(path).unwrap();
        std::fs::write(issuer_ready, b"ready").unwrap();
        wait_for_path(&revoke_complete);
        store
            .issue("issued".into(), TokenPermissions::ReadWrite, vec![], None)
            .unwrap();
        return;
    }

    wait_for_path(&issuer_ready);
    let store = TokenStore::open(path).unwrap();
    store.revoke(token_id).unwrap();
    std::fs::write(revoke_complete, b"revoked").unwrap();
}
