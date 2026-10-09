#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct Effects(u16);

impl Effects {
    pub(crate) const EMPTY: Self = Self(0);
    pub(crate) const READS: Self = Self(1 << 0);
    pub(crate) const WRITES_ROWS: Self = Self(1 << 1);
    pub(crate) const WRITES_SCHEMA: Self = Self(1 << 2);
    pub(crate) const ADMIN: Self = Self(1 << 3);
    pub(crate) const TRANSACTION_CONTROL: Self = Self(1 << 4);
    pub(crate) const SESSION_STATE: Self = Self(1 << 5);
    pub(crate) const HOST_OR_FILE_ACCESS: Self = Self(1 << 6);
    pub(crate) const UNKNOWN: Self = Self(1 << 7);

    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[cfg(test)]
    pub(crate) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[cfg(test)]
    pub(crate) const fn writes(self) -> bool {
        let write_effects = Self::WRITES_ROWS
            .union(Self::WRITES_SCHEMA)
            .union(Self::ADMIN)
            .union(Self::UNKNOWN);
        self.0 & write_effects.0 != 0
    }
}

#[cfg(test)]
mod tests {
    use crate::{classify::StatementClass, classify::classify_with_effects};

    use super::Effects;

    #[test]
    fn effects_match_legacy_write_facts_without_changing_legacy_classes() {
        for (sql, driver) in [
            ("SELECT * FROM users", "postgres"),
            ("SELECT * INTO backup FROM users", "postgres"),
            ("INSERT INTO users(id) VALUES (1)", "postgres"),
            ("UPDATE users SET id = 2 WHERE id = 1", "postgres"),
            ("DELETE FROM users WHERE id = 1", "postgres"),
            ("CREATE TABLE users(id INTEGER)", "postgres"),
            ("TRUNCATE TABLE users", "postgres"),
            ("COPY users TO PROGRAM 'echo unsafe'", "postgres"),
            (
                "MERGE INTO users USING staged ON users.id = staged.id WHEN MATCHED THEN DELETE",
                "postgres",
            ),
            ("SET work_mem = '64MB'", "postgres"),
            ("BEGIN", "postgres"),
            ("SELECT read_text('/etc/passwd')", "duckdb"),
        ] {
            let analysis = classify_with_effects(sql, driver);
            assert_eq!(analysis.effects.writes(), analysis.facts.writes, "SQL: {sql}");
        }

        let truncate = classify_with_effects("TRUNCATE TABLE users", "postgres");
        assert_eq!(truncate.facts.class, StatementClass::Ddl);
        assert!(truncate.effects.contains(Effects::WRITES_ROWS));
        assert!(!truncate.effects.contains(Effects::WRITES_SCHEMA));
    }

    #[test]
    fn effects_union_is_order_independent_while_legacy_script_class_remains_unchanged() {
        let delete_then_insert = classify_with_effects(
            "DELETE FROM users WHERE id = 1; INSERT INTO users(id) VALUES (2)",
            "postgres",
        );
        let insert_then_delete = classify_with_effects(
            "INSERT INTO users(id) VALUES (2); DELETE FROM users WHERE id = 1",
            "postgres",
        );

        assert_eq!(delete_then_insert.effects, insert_then_delete.effects);
        assert_eq!(delete_then_insert.facts.class, StatementClass::Insert);
        assert_eq!(insert_then_delete.facts.class, StatementClass::Delete);
        assert!(delete_then_insert.effects.contains(Effects::WRITES_ROWS));
    }

    #[test]
    fn effects_keep_security_and_session_facts_separate() {
        let copy = classify_with_effects("COPY users TO PROGRAM 'echo unsafe'", "postgres");
        assert!(copy.effects.contains(Effects::ADMIN));
        assert!(copy.effects.contains(Effects::HOST_OR_FILE_ACCESS));

        let begin = classify_with_effects("BEGIN", "postgres");
        assert!(begin.effects.contains(Effects::TRANSACTION_CONTROL));

        let set = classify_with_effects("SET work_mem = '64MB'", "postgres");
        assert!(set.effects.contains(Effects::SESSION_STATE));

        let file_read = classify_with_effects("SELECT read_text('/etc/passwd')", "duckdb");
        assert!(file_read.effects.contains(Effects::READS));
        assert!(file_read.effects.contains(Effects::HOST_OR_FILE_ACCESS));
        assert!(!file_read.effects.writes());

        let malformed = classify_with_effects("SELECCT * FROM users", "postgres");
        assert!(malformed.effects.contains(Effects::UNKNOWN));
    }

    #[test]
    fn effects_expose_side_effects_that_legacy_facts_do_not_record() {
        let mysql_lock = classify_with_effects("SELECT GET_LOCK('bookie', 1)", "mysql");
        assert_eq!(mysql_lock.facts.class, StatementClass::Select);
        assert!(!mysql_lock.facts.writes);
        assert!(mysql_lock.effects.contains(Effects::READS));
        assert!(mysql_lock.effects.contains(Effects::SESSION_STATE));
        assert!(mysql_lock.effects.contains(Effects::ADMIN));

        let duckdb_export = classify_with_effects("SELECT write_csv('users', '/tmp/users.csv')", "duckdb");
        assert_eq!(duckdb_export.facts.class, StatementClass::Select);
        assert!(!duckdb_export.facts.writes);
        assert!(duckdb_export.effects.contains(Effects::HOST_OR_FILE_ACCESS));
        assert!(duckdb_export.effects.contains(Effects::ADMIN));
    }

    #[test]
    fn effects_include_reads_used_to_build_or_mutate_rows() {
        let create_table = classify_with_effects("CREATE TABLE copy AS SELECT * FROM users", "postgres");
        assert!(create_table.effects.contains(Effects::READS));
        assert!(create_table.effects.contains(Effects::WRITES_SCHEMA));

        let insert = classify_with_effects("INSERT INTO copy SELECT * FROM users", "postgres");
        assert!(insert.effects.contains(Effects::READS));
        assert!(insert.effects.contains(Effects::WRITES_ROWS));
    }
}
