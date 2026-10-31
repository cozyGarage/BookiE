#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::Environment;
use tablepro_policy::{Decision, PolicyConfig, Principal, WritePolicy, classify, evaluate};

fn agent() -> Principal {
    Principal::Agent {
        token: "test-token".into(),
        client: None,
        model: None,
    }
}

fn decide(principal: &Principal, sql: &str, driver_id: &str, read_only: bool, agent_writes: WritePolicy) -> Decision {
    let mut config = PolicyConfig::default();
    config.environments.entry("local".into()).or_default().agent_writes = Some(agent_writes);
    evaluate(
        principal,
        Environment::Local,
        &classify(sql, driver_id),
        read_only,
        &config.for_environment(Environment::Local),
        None,
    )
}

const LOCKING_READS: [(&str, &str); 4] = [
    ("SELECT * FROM t WHERE id = 1 FOR UPDATE", "postgres"),
    ("SELECT * FROM t FOR SHARE", "postgres"),
    ("SELECT * FROM t WHERE id = 1 FOR UPDATE", "mysql"),
    ("SELECT * FROM t LOCK IN SHARE MODE", "mysql"),
];

#[test]
fn a_locking_read_is_denied_on_a_read_only_connection() {
    for (sql, driver) in LOCKING_READS {
        let decision = decide(&agent(), sql, driver, true, WritePolicy::Allow);
        assert!(
            matches!(decision, Decision::Deny { .. }),
            "{driver}: {sql}: {decision:?}"
        );
    }
}

#[test]
fn an_agent_that_may_not_write_may_not_take_row_locks() {
    for (sql, driver) in LOCKING_READS {
        let decision = decide(&agent(), sql, driver, false, WritePolicy::Deny);
        assert!(
            matches!(decision, Decision::Deny { .. }),
            "{driver}: {sql}: {decision:?}"
        );
    }
}

#[test]
fn a_plain_read_still_passes_both_checks() {
    let decision = decide(&agent(), "SELECT * FROM t", "postgres", true, WritePolicy::Deny);
    assert!(matches!(decision, Decision::Allow { .. }), "{decision:?}");
}
