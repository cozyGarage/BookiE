#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tablepro_core::Environment;
use tablepro_policy::{Decision, EnvPolicy, PolicyConfig, Principal, WritePolicy, classify, evaluate};

fn agent() -> Principal {
    Principal::Agent {
        token: "token".into(),
        client: None,
        model: None,
    }
}

fn policy(agent_writes: WritePolicy, multi_statement: bool) -> EnvPolicy {
    let mut config = PolicyConfig::default();
    let local = config.environments.entry("local".into()).or_default();
    local.agent_writes = Some(agent_writes);
    local.agent_allow_multi_statement = Some(multi_statement);
    config.for_environment(Environment::Local)
}

fn denied(sql: &str, driver_id: &str, agent_writes: WritePolicy, multi_statement: bool) -> bool {
    matches!(
        evaluate(
            &agent(),
            Environment::Local,
            &classify(sql, driver_id),
            false,
            &policy(agent_writes, multi_statement),
            None,
        ),
        Decision::Deny { .. }
    )
}

#[test]
fn an_agent_with_writes_allowed_is_still_denied_host_and_administrative_reach() {
    for (sql, driver) in [
        ("SELECT load_extension('x')", "sqlite"),
        ("ATTACH DATABASE '/tmp/a.db' AS a", "sqlite"),
        ("LOAD DATA LOCAL INFILE '/etc/passwd' INTO TABLE t", "mysql"),
        ("SELECT * FROM t INTO OUTFILE '/tmp/x'", "mysql"),
        ("COPY t TO PROGRAM 'cat'", "postgres"),
        ("COPY t FROM '/etc/passwd'", "postgres"),
        ("DO $$ BEGIN PERFORM 1; END $$", "postgres"),
        ("EXEC xp_cmdshell 'dir'", "mssql"),
        ("SELECT pg_terminate_backend(1)", "postgres"),
        ("SELECT 1; SELECT pg_terminate_backend(1)", "postgres"),
    ] {
        assert!(denied(sql, driver, WritePolicy::Allow, true), "{driver}: {sql}");
    }
}

#[test]
fn an_agent_without_write_permission_cannot_run_a_write_hidden_in_another_form() {
    for (sql, driver) in [
        ("EXPLAIN ANALYZE DELETE FROM t WHERE id = 1", "postgres"),
        ("EXPLAIN (ANALYZE) DELETE FROM t WHERE id = 1", "postgres"),
        ("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d", "postgres"),
        ("SELECT 1; DELETE FROM t", "postgres"),
        ("/* a */ DELETE /* b */ FROM t", "mysql"),
    ] {
        assert!(denied(sql, driver, WritePolicy::Deny, true), "{driver}: {sql}");
    }
}

#[test]
fn an_agent_may_read_when_writes_are_denied() {
    for (sql, driver) in [
        ("SELECT 1", "postgres"),
        ("EXPLAIN SELECT * FROM t", "postgres"),
        ("EXPLAIN (ANALYZE false) DELETE FROM t", "postgres"),
        ("SELECT * FROM t WHERE name = 'DELETE FROM x'", "mysql"),
        ("SELECT /* DROP TABLE t */ 1", "sqlite"),
    ] {
        assert!(!denied(sql, driver, WritePolicy::Deny, false), "{driver}: {sql}");
    }
}

#[test]
fn a_multi_statement_script_is_denied_unless_the_environment_allows_it() {
    assert!(denied("SELECT 1; SELECT 2", "postgres", WritePolicy::Allow, false));
    assert!(!denied("SELECT 1; SELECT 2", "postgres", WritePolicy::Allow, true));
}

#[test]
fn text_that_cannot_be_parsed_is_not_a_plain_read_for_an_agent() {
    for (sql, driver) in [
        ("SELECT mysql_func /*!50000 , sleep(5) */ 1", "mysql"),
        ("SELEC 1 FRM", "postgres"),
        ("", "postgres"),
    ] {
        assert!(denied(sql, driver, WritePolicy::Deny, false), "{driver}: {sql:?}");
    }
}
