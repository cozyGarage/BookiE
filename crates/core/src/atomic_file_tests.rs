use std::os::unix::fs::PermissionsExt;

use super::replace_private_file;

#[test]
fn the_file_is_replaced_whole_and_only_the_owner_can_read_it() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("settings.json");
    std::fs::write(&target, b"old").unwrap();
    replace_private_file(&target, &dir.path().join(".settings.json.tmp"), b"new").unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert!(!dir.path().join(".settings.json.tmp").exists());
}

#[test]
fn an_existing_temporary_file_is_refused_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("settings.json");
    let temporary = dir.path().join(".settings.json.tmp");
    std::fs::write(&temporary, b"someone else's file").unwrap();
    assert!(replace_private_file(&target, &temporary, b"new").is_err());
    assert_eq!(std::fs::read(&temporary).unwrap(), b"someone else's file");
    assert!(!target.exists());
}
