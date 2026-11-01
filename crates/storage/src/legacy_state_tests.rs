use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::*;

fn write_private(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn legacy_state_is_copied_forward_and_the_original_is_kept() {
    let base = tempfile::tempdir().unwrap();
    write_private(&base.path().join("tablepro/connections.json"), "saved");
    write_private(&base.path().join("tablepro/drafts/a.sql"), "draft");

    assert_eq!(migrate_legacy_directories(base.path()).unwrap(), 1);

    assert_eq!(
        fs::read_to_string(base.path().join("bookie/connections.json")).unwrap(),
        "saved"
    );
    assert_eq!(
        fs::read_to_string(base.path().join("bookie/drafts/a.sql")).unwrap(),
        "draft"
    );
    assert_eq!(
        fs::read_to_string(base.path().join("tablepro/connections.json")).unwrap(),
        "saved"
    );
    let mode = fs::metadata(base.path().join("bookie/connections.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn existing_new_state_is_never_overwritten() {
    let base = tempfile::tempdir().unwrap();
    write_private(&base.path().join("tablepro/connections.json"), "legacy");
    write_private(&base.path().join("bookie/connections.json"), "current");

    assert_eq!(migrate_legacy_directories(base.path()).unwrap(), 0);

    assert_eq!(
        fs::read_to_string(base.path().join("bookie/connections.json")).unwrap(),
        "current"
    );
}

#[test]
fn the_development_profile_directory_migrates_independently() {
    let base = tempfile::tempdir().unwrap();
    write_private(&base.path().join("tablepro-devel/history.db"), "devel");

    assert_eq!(migrate_legacy_directories(base.path()).unwrap(), 1);

    assert_eq!(
        fs::read_to_string(base.path().join("bookie-devel/history.db")).unwrap(),
        "devel"
    );
    assert!(!base.path().join("bookie").exists());
}

#[test]
fn symlinks_in_legacy_state_are_not_followed() {
    let base = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write_private(&outside.path().join("secret"), "outside");
    write_private(&base.path().join("tablepro/kept.json"), "kept");
    std::os::unix::fs::symlink(outside.path().join("secret"), base.path().join("tablepro/link")).unwrap();

    migrate_legacy_directories(base.path()).unwrap();

    assert!(base.path().join("bookie/kept.json").exists());
    assert!(!base.path().join("bookie/link").exists());
}

#[test]
fn a_symlinked_legacy_directory_is_ignored() {
    let base = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write_private(&outside.path().join("connections.json"), "outside");
    std::os::unix::fs::symlink(outside.path(), base.path().join("tablepro")).unwrap();

    assert_eq!(migrate_legacy_directories(base.path()).unwrap(), 0);
    assert!(!base.path().join("bookie").exists());
}

#[test]
fn the_legacy_keyring_schema_follows_the_current_one() {
    assert_eq!(
        legacy_secret_schema("com.bookie.linux.Password"),
        "com.tablepro.linux.Password"
    );
    assert_eq!(
        legacy_secret_schema("com.bookie.linux.Devel.Password"),
        "com.tablepro.linux.Devel.Password"
    );
}
