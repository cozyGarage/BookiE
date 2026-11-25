use bookie_core::{DatabaseDriver, Value};
use testcontainers::core::ExecCommand;

use super::{CassandraDriver, seed, start_cassandra};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_simple_select_stays_within_its_server_command_budget() {
    let (container, mut options) = start_cassandra().await;
    seed(&container).await;
    options.database = "conf".into();
    let connection = CassandraDriver.connect(options).await.expect("connect BookiE");
    let query = "SELECT label FROM conf.items WHERE id = 1";
    let warmup = connection.query(query).await.expect("warm BookiE connection");
    assert_eq!(warmup.rows, vec![vec![Value::Text("one".into())]]);

    let mut make_audit_dir = container
        .exec(ExecCommand::new(["mkdir", "-p", "/opt/cassandra/logs/audit"]))
        .await
        .expect("create Cassandra audit log directory");
    let audit_dir_output = make_audit_dir.stdout_to_vec().await.expect("audit directory output");
    assert_eq!(
        make_audit_dir.exit_code().await.expect("audit directory exit code"),
        Some(0),
        "create Cassandra audit log directory: {audit_dir_output:?}"
    );

    let mut enable = container
        .exec(ExecCommand::new([
            "nodetool",
            "enableauditlog",
            "--included-keyspaces",
            "conf",
            "--included-categories",
            "QUERY",
            "--logger",
            "BinAuditLogger",
        ]))
        .await
        .expect("enable Cassandra audit log");
    let enable_output = enable.stdout_to_vec().await.expect("audit setup output");
    let enable_error = enable.stderr_to_vec().await.expect("audit setup error");
    assert_eq!(
        enable.exit_code().await.expect("audit setup exit code"),
        Some(0),
        "stdout: {enable_output:?}\nstderr: {enable_error:?}"
    );

    let result = connection.query(query).await.expect("run measured SELECT");
    assert_eq!(result.rows, vec![vec![Value::Text("one".into())]]);

    let mut audit_log = container
        .exec(ExecCommand::new([
            "/opt/cassandra/tools/bin/auditlogviewer",
            "/opt/cassandra/logs/audit",
        ]))
        .await
        .expect("read Cassandra audit log");
    let output = String::from_utf8(audit_log.stdout_to_vec().await.expect("audit log output"))
        .expect("UTF-8 Cassandra audit log");
    let error = String::from_utf8(audit_log.stderr_to_vec().await.expect("audit log error"))
        .expect("UTF-8 Cassandra audit log error");
    assert_eq!(
        audit_log.exit_code().await.expect("audit log exit code"),
        Some(0),
        "decode Cassandra audit log: {output}\n{error}"
    );
    let statements = output
        .lines()
        .filter(|line| {
            line.contains("category:QUERY")
                && line.contains("conf")
                && line.contains("items")
                && line.contains("SELECT")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        statements.len(),
        1,
        "simple SELECT expected one audited CQL command, observed:\n{}",
        output
    );
}
