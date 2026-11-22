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

    let mut enable = container
        .exec(ExecCommand::new([
            "nodetool",
            "enableauditlog",
            "--included-keyspaces",
            "conf",
            "--included-categories",
            "QUERY",
            "--logger",
            "FileAuditLogger",
        ]))
        .await
        .expect("enable Cassandra audit log");
    let enable_output = enable.stdout_to_vec().await.expect("audit setup output");
    assert_eq!(
        enable.exit_code().await.expect("audit setup exit code"),
        Some(0),
        "{enable_output:?}"
    );

    let result = connection.query(query).await.expect("run measured SELECT");
    assert_eq!(result.rows, vec![vec![Value::Text("one".into())]]);

    let output = String::from_utf8(container.stdout_to_vec().await.expect("Cassandra server logs"))
        .expect("UTF-8 Cassandra server logs");
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
