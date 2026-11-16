#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{ActivityQuery, Environment, activity_kinds, activity_sql};
use bookie_policy::{Decision, PolicyConfig, Principal, StatementClass, classify, evaluate};

const DRIVERS: [&str; 3] = ["postgres", "mysql", "mssql"];
const ENVIRONMENTS: [Environment; 3] = [Environment::Local, Environment::Staging, Environment::Prod];

fn read_kinds(driver: &str) -> impl Iterator<Item = ActivityQuery> {
    activity_kinds(driver)
        .iter()
        .copied()
        .filter(|kind| *kind != ActivityQuery::KillSession)
}

fn agent() -> Principal {
    Principal::Agent {
        token: "test".into(),
        client: None,
        model: None,
    }
}

#[test]
fn server_variables_and_status_are_among_the_statements_under_test() {
    for driver in ["postgres", "mysql"] {
        let kinds: Vec<_> = read_kinds(driver).collect();
        assert!(kinds.contains(&ActivityQuery::ServerVariables), "{driver}");
        assert!(kinds.contains(&ActivityQuery::ServerStatus), "{driver}");
    }
}

#[test]
fn every_server_activity_read_is_classified_as_a_plain_read() {
    for driver in DRIVERS {
        for kind in read_kinds(driver) {
            let sql = activity_sql(driver, kind, None).unwrap();
            let facts = classify(&sql, driver);
            assert_eq!(facts.class, StatementClass::Select, "{driver} {kind:?}: {sql}");
            assert!(!facts.writes(), "{driver} {kind:?} must not classify as a write: {sql}");
        }
    }
}

#[test]
fn every_server_activity_read_is_allowed_for_people_and_agents_on_read_only_connections() {
    let config = PolicyConfig::default();
    for driver in DRIVERS {
        for kind in read_kinds(driver) {
            let sql = activity_sql(driver, kind, None).unwrap();
            let facts = classify(&sql, driver);
            for environment in ENVIRONMENTS {
                let policy = config.for_environment(environment);
                for principal in [Principal::human_gui(), agent()] {
                    let decision = evaluate(&principal, environment, &facts, true, &policy, None);
                    assert!(
                        matches!(decision, Decision::Allow { .. }),
                        "{driver} {kind:?} {environment:?}: {decision:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn killing_a_session_is_not_a_plain_read() {
    for driver in DRIVERS {
        let sql = activity_sql(driver, ActivityQuery::KillSession, Some(42)).unwrap();
        let facts = classify(&sql, driver);
        let policy = PolicyConfig::default().for_environment(Environment::Prod);
        let decision = evaluate(&Principal::human_gui(), Environment::Prod, &facts, true, &policy, None);
        assert!(
            !matches!(decision, Decision::Allow { .. }),
            "{driver} kill on a read-only production connection: {decision:?}"
        );
    }
}
