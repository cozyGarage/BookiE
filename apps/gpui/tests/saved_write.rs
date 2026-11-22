#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::process::Command;

#[test]
fn saved_sqlite_write_uses_isolated_profile_and_audit() {
    if std::env::var_os("BOOKIE_SAVED_WRITE_CHILD").is_some() {
        run_saved_write();
        return;
    }
    let root = std::env::temp_dir().join(format!("bookie-saved-write-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("saved_sqlite_write_uses_isolated_profile_and_audit")
        .env("BOOKIE_SAVED_WRITE_CHILD", "1")
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("BOOKIE_SAVED_WRITE_ROOT", &root)
        .status()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(status.success());
}

fn run_saved_write() {
    let root = std::path::PathBuf::from(std::env::var_os("BOOKIE_SAVED_WRITE_ROOT").unwrap());
    let database = root.join("orders.sqlite");
    let fixture =
        std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("tests/fixtures/orders.sqlite");
    std::fs::copy(fixture, &database).unwrap();
    let file = bookie_app_services::open_request::classify_file(&database).unwrap();
    let saved = bookie_app_services::open_request::file_connection(&file);
    let mut readonly = saved.clone();
    readonly.id = uuid::Uuid::new_v4();
    readonly.read_only = true;
    let mut production = saved.clone();
    production.id = uuid::Uuid::new_v4();
    production.environment = bookie_core::Environment::Prod;
    let connection_id = saved.id;
    let readonly_id = readonly.id;
    let production_id = production.id;
    let profile = if option_env!("BOOKIE_PROFILE") == Some("development") {
        "bookie-devel"
    } else {
        "bookie"
    };
    let config = root.join("config").join(profile);
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("connections.json"),
        serde_json::to_vec(&serde_json::json!({"version": 2, "connections": [saved, readonly, production]})).unwrap(),
    )
    .unwrap();
    let (sender, _receiver) = bookie_app_services::approval_prompt::prompt_channel();
    let session = bookie_gpui::live::WriteSession::open_saved(connection_id, sender, None).unwrap();
    session
        .apply(
            &[("UPDATE orders SET customer = 'saved test' WHERE id = 1".into(), vec![])],
            &[0],
            &tokio_util::sync::CancellationToken::new(),
        )
        .unwrap_or_else(|error| panic!("{}", error.message));
    let (sender, _receiver) = bookie_app_services::approval_prompt::prompt_channel();
    let session = bookie_gpui::live::WriteSession::open_saved(readonly_id, sender, None).unwrap();
    let denied = session.apply(
        &[("UPDATE orders SET customer = 'forbidden' WHERE id = 1".into(), vec![])],
        &[0],
        &tokio_util::sync::CancellationToken::new(),
    );
    assert!(denied.is_err(), "read-only connection accepted a write");

    let (sender, mut receiver) = bookie_app_services::approval_prompt::prompt_channel();
    let session = bookie_gpui::live::WriteSession::open_saved(production_id, sender, None).unwrap();
    let responder = std::thread::spawn(move || {
        let prompt = receiver.blocking_recv().expect("production write needs approval");
        assert_eq!(prompt.request.environment, bookie_core::Environment::Prod);
        assert_eq!(prompt.request.connection_id, production_id);
        prompt
            .respond
            .send(bookie_app_services::approval_prompt::ApprovalOutcome::Deny)
            .unwrap();
    });
    let denied = session.apply(
        &[("UPDATE orders SET customer = 'denied' WHERE id = 1".into(), vec![])],
        &[0],
        &tokio_util::sync::CancellationToken::new(),
    );
    drop(session);
    responder.join().unwrap();
    assert!(denied.is_err(), "denied production write succeeded");

    use bookie_gpui::model::CellText;
    use bookie_gpui::source::ResultSource;
    let reader = bookie_gpui::live::LiveSession::open(&bookie_gpui::live::Origin::File(database)).unwrap();
    let result = reader.run_query("SELECT customer FROM orders WHERE id = 1", 1).unwrap();
    assert_eq!(result.row(0)[0].display(), "saved test");

    let audit = root.join("data").join(profile).join("audit.jsonl");
    let records: Vec<serde_json::Value> = std::fs::read_to_string(audit)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for (id, status) in [
        (connection_id, "succeeded"),
        (readonly_id, "denied"),
        (production_id, "denied"),
    ] {
        assert!(
            records.iter().any(|record| {
                record["event"]["connection_id"] == id.to_string() && record["event"]["terminal_status"] == status
            }),
            "missing {status} audit for {id}: {records:?}"
        );
    }
}
