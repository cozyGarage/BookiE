#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tablepro_policy::{StatementClass, classify};

#[test]
fn a_redis_select_switches_databases_and_is_not_a_read() {
    for command in ["SELECT 2", "select 0", "GET key", "FLUSHDB"] {
        let facts = classify(command, "redis");
        assert_eq!(facts.class, StatementClass::Unparseable, "{command}");
        assert!(facts.writes(), "{command}");
    }
    assert_eq!(classify("SELECT 2", "postgres").class, StatementClass::Select);
}
