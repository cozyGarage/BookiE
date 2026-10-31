#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use tablepro_policy::classify;

fn tables(sql: &str, driver_id: &str) -> Vec<String> {
    let mut names = classify(sql, driver_id).tables;
    names.sort();
    names.dedup();
    names
}

#[test]
fn an_update_names_every_table_it_joins() {
    assert_eq!(
        tables("UPDATE a JOIN b ON a.id = b.id SET b.x = 1 WHERE a.id = 1", "mysql"),
        vec!["a", "b"]
    );
    assert_eq!(
        tables("UPDATE a SET x = 1 FROM b WHERE a.id = b.id", "postgres"),
        vec!["a", "b"]
    );
}

#[test]
fn a_delete_names_every_table_it_joins_or_uses() {
    assert_eq!(
        tables("DELETE a FROM a JOIN b ON a.id = b.id WHERE b.x = 1", "mysql"),
        vec!["a", "b"]
    );
    assert_eq!(
        tables(
            "DELETE t1, t2 FROM t1 INNER JOIN t2 INNER JOIN t3 WHERE t1.id = t2.id",
            "mysql"
        ),
        vec!["t1", "t2", "t3"]
    );
    assert_eq!(
        tables("DELETE FROM a USING b WHERE a.id = b.id", "postgres"),
        vec!["a", "b"]
    );
}

#[test]
fn single_table_statements_are_unchanged() {
    assert_eq!(
        tables("UPDATE users SET name = 'x' WHERE id = 1", "mysql"),
        vec!["users"]
    );
    assert_eq!(tables("DELETE FROM users WHERE id = 1", "postgres"), vec!["users"]);
}
