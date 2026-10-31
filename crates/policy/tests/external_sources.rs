#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tablepro_core::Environment;
use tablepro_policy::{Decision, PolicyConfig, Principal, StatementClass, classify, evaluate};

fn agent_is_denied(sql: &str, driver_id: &str) -> bool {
    let facts = classify(sql, driver_id);
    let decision = evaluate(
        &Principal::Agent {
            token: "token".into(),
            client: None,
            model: None,
        },
        Environment::Local,
        &facts,
        false,
        &PolicyConfig::default().for_environment(Environment::Local),
        None,
    );
    matches!(decision, Decision::Deny { .. })
}

#[test]
fn every_clickhouse_table_function_that_leaves_the_server_is_administrative() {
    for function in [
        "azureBlobStorageCluster('conn', 'c', 'f')",
        "deltaLakeCluster('https://x/y')",
        "hudiCluster('c', 'https://x/y')",
        "icebergCluster('https://x/y')",
        "redis('host:6379', 'k', 'k String')",
        "executable('run.sh', TSV, 'a String')",
        "gcs('https://storage.googleapis.com/b/f', 'CSV')",
        "oss('https://b.oss-cn.aliyuncs.com/f', 'CSV')",
        "cosn('https://b.cos.example.com/f', 'CSV')",
        "hive('thrift://h:9083', 'db', 't', 'a String', 'a')",
        "arrowFlight('host:8888', 'dataset')",
        "fileCluster('c', 'x.csv', 'CSV')",
    ] {
        let sql = format!("SELECT * FROM {function}");
        assert_eq!(
            classify(&sql, "clickhouse").class,
            StatementClass::Administrative,
            "{sql}"
        );
        assert!(agent_is_denied(&sql, "clickhouse"), "{sql}");
    }
}

#[test]
fn postgres_functions_that_create_replication_slots_are_administrative() {
    for sql in [
        "SELECT pg_copy_logical_replication_slot('a', 'b')",
        "SELECT pg_copy_physical_replication_slot('a', 'b')",
        "SELECT pg_create_logical_replication_slot('a', 'pgoutput')",
        "SELECT pg_drop_replication_slot('a')",
    ] {
        assert!(agent_is_denied(sql, "postgres"), "{sql}");
    }
}

#[test]
fn a_column_named_like_an_external_source_stays_a_read() {
    assert!(!agent_is_denied("SELECT executable, redis FROM tools", "clickhouse"));
}
