#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The shipped daemon, a real SQLite database and a client that sends no token of its own.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use bookie_core::{AuthMode, ConnectOptions, DatabaseDriver, Environment, TlsMode};
use bookie_mcp::{TokenPermissions, TokenStore};
use bookie_storage::{SavedConnection, save_connections};
use serde_json::{Value, json};
use uuid::Uuid;

fn daemon_binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_bookie-agentd")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_bookie-agentd").into())
        .into()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_without_a_token_reads_a_real_database_through_the_daemons_environment_token() {
    let root = tempfile::tempdir().unwrap();
    for (variable, directory) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        let path = root.path().join(directory);
        std::fs::create_dir_all(&path).unwrap();
        if variable == "XDG_RUNTIME_DIR" {
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        }
        // SAFETY: this file holds one test and sets the variables before any other thread reads them.
        unsafe { std::env::set_var(variable, &path) };
    }

    let database = root.path().join("people.sqlite");
    std::fs::File::create(&database).unwrap();
    let writer = drivers_sqlite::SqliteDriver
        .connect(ConnectOptions {
            database: database.to_string_lossy().into_owned(),
            ..ConnectOptions::default()
        })
        .await
        .unwrap();
    writer
        .execute("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .unwrap();
    writer
        .execute("INSERT INTO people VALUES (1, 'Ada Lovelace'), (2, 'Grace Hopper')")
        .await
        .unwrap();

    let saved = SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "People".into(),
        driver_id: "sqlite".into(),
        host: String::new(),
        port: 0,
        socket_dir: None,
        socket_path: None,
        database: database.to_string_lossy().into_owned(),
        username: String::new(),
        use_tls: false,
        tls_mode: Some(TlsMode::Disabled),
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: true,
        auth_mode: AuthMode::Password,
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
        attached_databases: Vec::new(),
        tls_strict_encryption: false,
    };
    save_connections(std::slice::from_ref(&saved)).await.unwrap();
    let tokens = TokenStore::open_default().unwrap();
    let (_, token) = tokens
        .issue("env-token".into(), TokenPermissions::ReadOnly, vec![saved.id], None)
        .unwrap();
    let policy = root.path().join("policy.toml");
    std::fs::write(&policy, "").unwrap();

    let connection_id = saved.id;
    let answers = tokio::task::spawn_blocking(move || {
        let mut daemon = Command::new(daemon_binary())
            .arg("--policy")
            .arg(&policy)
            .env("BOOKIE_MCP_TOKEN", &token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = daemon.stdin.take().unwrap();
        let mut stdout = BufReader::new(daemon.stdout.take().unwrap());
        let mut ask = |request: Value| -> Value {
            writeln!(stdin, "{request}").unwrap();
            let mut line = String::new();
            stdout.read_line(&mut line).unwrap();
            serde_json::from_str(&line).unwrap_or_else(|_| panic!("not JSON: {line:?}"))
        };
        let listed = ask(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}));
        let queried = ask(json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"execute_query","arguments":{
                "connection_id": connection_id,
                "sql":"SELECT name FROM people ORDER BY id"}}
        }));
        drop(stdin);
        let _ = daemon.wait();
        (listed, queried)
    })
    .await
    .unwrap();

    let (listed, queried) = answers;
    assert!(
        listed["result"]["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty()),
        "{listed}"
    );
    let text = queried["result"]["content"][0]["text"].as_str().unwrap_or_default();
    assert_eq!(queried["result"]["isError"], false, "{queried}");
    assert!(text.contains("Ada Lovelace") && text.contains("Grace Hopper"), "{text}");
}
