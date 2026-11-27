use crate::save_ui_tests::{saved_customer, write_session};
use crate::ui_tests::test_kit::orders;

#[test]
fn tests_keep_secrets_and_state_files_away_from_the_users_session() {
    assert!(bookie_app_services::isolated_test_state_active());
    let dir = bookie_app_services::isolated_test_state_dir().expect("an isolated state directory");
    assert!(dir.starts_with(std::env::temp_dir()), "{}", dir.display());
    let policy = bookie_app_services::config_io::xdg_config_path("policy.toml").expect("a policy path");
    assert!(policy.starts_with(dir), "{}", policy.display());
}

#[test]
fn the_app_keeps_secrets_and_state_files_in_the_users_session() {
    let source = include_str!("../main.rs");
    let body = source.split("\nfn main() {\n").nth(1).expect("main exists");
    let second = body.lines().nth(1).expect("main has a body").trim();
    assert_eq!(second, "bookie_app_services::use_user_state();");
}

#[test]
fn an_audited_write_needs_no_secret_service_and_writes_no_key_file() {
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/bookie-test-bus");
    }
    let path = orders();
    let write = write_session(&path, false);

    write
        .apply(
            &[("UPDATE orders SET customer = 'Memory' WHERE id = 1".into(), Vec::new())],
            &[0],
            &tokio_util::sync::CancellationToken::new(),
        )
        .ok()
        .expect("an audited write");

    assert_eq!(saved_customer(&path, 1), "Memory");
    let data = bookie_app_services::isolated_test_state_dir()
        .expect("an isolated state directory")
        .join("data");
    for profile in ["bookie", "bookie-devel"] {
        let key_file = data.join(profile).join("audit-sql-hash-key");
        assert!(!key_file.exists(), "{}", key_file.display());
    }
}
