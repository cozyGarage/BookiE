use crate::Value;
use crate::sql_dialect::quote_ident;

pub const HEALTH_ROW_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthReport {
    TableVacuum,
    EstimatedBloat,
    UnusedIndexes,
    DuplicateIndexes,
    SeqScanHeavy,
}

impl HealthReport {
    pub const ALL: [Self; 5] = [
        Self::TableVacuum,
        Self::EstimatedBloat,
        Self::UnusedIndexes,
        Self::DuplicateIndexes,
        Self::SeqScanHeavy,
    ];

    pub fn sql(self) -> String {
        match self {
            Self::TableVacuum => table_vacuum_sql(),
            Self::EstimatedBloat => estimated_bloat_sql(),
            Self::UnusedIndexes => unused_indexes_sql(),
            Self::DuplicateIndexes => duplicate_indexes_sql(),
            Self::SeqScanHeavy => seq_scan_heavy_sql(),
        }
    }

    pub fn diagnostic_sql(self, schema: &str, object: &str) -> Option<String> {
        let target = format!(
            "{}.{}",
            quote_ident("postgres", schema),
            quote_ident("postgres", object)
        );
        match self {
            Self::TableVacuum | Self::EstimatedBloat => Some(format!("VACUUM (ANALYZE) {target}")),
            Self::UnusedIndexes | Self::DuplicateIndexes => Some(format!("DROP INDEX CONCURRENTLY IF EXISTS {target}")),
            Self::SeqScanHeavy => None,
        }
    }
}

pub fn hidden_schemas_param(hidden: &[String]) -> Value {
    Value::Text(serde_json::to_string(hidden).unwrap_or_else(|_| "[]".to_owned()))
}

const HIDDEN_FILTER: &str = "NOT EXISTS (SELECT 1 FROM pg_catalog.jsonb_array_elements_text($1::text::jsonb) \
     AS hidden(schema_name) WHERE hidden.schema_name = ";

fn visible_stat_table(alias: &str) -> String {
    format!(
        "pg_catalog.has_schema_privilege({alias}.schemaname, 'USAGE') \
         AND pg_catalog.has_any_column_privilege({alias}.relid, 'SELECT,INSERT,UPDATE,REFERENCES') \
         AND {HIDDEN_FILTER}{alias}.schemaname)"
    )
}

fn table_vacuum_sql() -> String {
    format!(
        "SELECT s.schemaname AS schema_name, s.relname AS table_name, \
         pg_catalog.pg_total_relation_size(s.relid) AS total_bytes, \
         pg_catalog.pg_size_pretty(pg_catalog.pg_total_relation_size(s.relid)) AS total_size, \
         s.n_live_tup AS live_rows, s.n_dead_tup AS dead_rows, \
         round(100.0 * s.n_dead_tup / nullif(s.n_live_tup + s.n_dead_tup, 0), 1) AS dead_percent, \
         greatest(s.last_vacuum, s.last_autovacuum)::text AS last_vacuum, \
         greatest(s.last_analyze, s.last_autoanalyze)::text AS last_analyze \
         FROM pg_catalog.pg_stat_user_tables s \
         WHERE s.n_dead_tup > 0 AND {} \
         ORDER BY s.n_dead_tup DESC, s.schemaname, s.relname LIMIT {HEALTH_ROW_LIMIT}",
        visible_stat_table("s")
    )
}

fn estimated_bloat_sql() -> String {
    format!(
        "SELECT s.schemaname AS schema_name, s.relname AS table_name, \
         pg_catalog.pg_relation_size(s.relid) AS table_bytes, \
         round(pg_catalog.pg_relation_size(s.relid) * s.n_dead_tup::numeric \
               / nullif(s.n_live_tup + s.n_dead_tup, 0)) AS estimated_bloat_bytes, \
         pg_catalog.pg_size_pretty(round(pg_catalog.pg_relation_size(s.relid) * s.n_dead_tup::numeric \
               / nullif(s.n_live_tup + s.n_dead_tup, 0))) AS estimated_bloat, \
         round(100.0 * s.n_dead_tup / nullif(s.n_live_tup + s.n_dead_tup, 0), 1) AS dead_percent \
         FROM pg_catalog.pg_stat_user_tables s \
         WHERE s.n_dead_tup > 0 AND pg_catalog.pg_relation_size(s.relid) >= 1048576 AND {} \
         ORDER BY estimated_bloat_bytes DESC NULLS LAST, s.schemaname, s.relname LIMIT {HEALTH_ROW_LIMIT}",
        visible_stat_table("s")
    )
}

fn unused_indexes_sql() -> String {
    format!(
        "SELECT s.schemaname AS schema_name, s.indexrelname AS index_name, s.relname AS table_name, \
         pg_catalog.pg_relation_size(s.indexrelid) AS index_bytes, \
         pg_catalog.pg_size_pretty(pg_catalog.pg_relation_size(s.indexrelid)) AS index_size, \
         s.idx_scan AS scans, pg_catalog.pg_get_indexdef(s.indexrelid) AS definition, \
         (SELECT d.stats_reset::text FROM pg_catalog.pg_stat_database d \
          WHERE d.datname = pg_catalog.current_database()) AS statistics_since \
         FROM pg_catalog.pg_stat_user_indexes s \
         JOIN pg_catalog.pg_index i ON i.indexrelid = s.indexrelid \
         WHERE s.idx_scan = 0 AND NOT i.indisunique AND NOT i.indisprimary AND NOT i.indisexclusion \
         AND {} \
         ORDER BY pg_catalog.pg_relation_size(s.indexrelid) DESC, s.schemaname, s.indexrelname \
         LIMIT {HEALTH_ROW_LIMIT}",
        visible_stat_table("s")
    )
}

fn duplicate_indexes_sql() -> String {
    format!(
        "SELECT n.nspname AS schema_name, ic.relname AS index_name, c.relname AS table_name, \
         bc.relname AS covered_by, pg_catalog.pg_relation_size(a.indexrelid) AS index_bytes, \
         pg_catalog.pg_size_pretty(pg_catalog.pg_relation_size(a.indexrelid)) AS index_size, \
         pg_catalog.pg_get_indexdef(a.indexrelid) AS definition \
         FROM pg_catalog.pg_index a \
         JOIN pg_catalog.pg_index b ON b.indrelid = a.indrelid AND b.indexrelid <> a.indexrelid \
         JOIN pg_catalog.pg_class ic ON ic.oid = a.indexrelid \
         JOIN pg_catalog.pg_class bc ON bc.oid = b.indexrelid \
         JOIN pg_catalog.pg_class c ON c.oid = a.indrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE NOT a.indisunique AND NOT a.indisprimary AND NOT a.indisexclusion \
         AND a.indisvalid AND b.indisvalid AND a.indnkeyatts = a.indnatts \
         AND a.indexprs IS NULL AND b.indexprs IS NULL \
         AND ic.relam = bc.relam \
         AND pg_catalog.pg_get_expr(a.indpred, a.indrelid) IS NOT DISTINCT FROM pg_catalog.pg_get_expr(b.indpred, b.indrelid) \
         AND (b.indkey::text = a.indkey::text OR b.indkey::text LIKE a.indkey::text || ' %') \
         AND (b.indclass::text = a.indclass::text OR b.indclass::text LIKE a.indclass::text || ' %') \
         AND (b.indoption::text = a.indoption::text OR b.indoption::text LIKE a.indoption::text || ' %') \
         AND (b.indkey::text <> a.indkey::text OR a.indexrelid > b.indexrelid) \
         AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
         AND n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp\\_%' \
         AND pg_catalog.has_schema_privilege(n.oid, 'USAGE') \
         AND pg_catalog.has_any_column_privilege(c.oid, 'SELECT,INSERT,UPDATE,REFERENCES') \
         AND {HIDDEN_FILTER}n.nspname) \
         ORDER BY pg_catalog.pg_relation_size(a.indexrelid) DESC, n.nspname, ic.relname \
         LIMIT {HEALTH_ROW_LIMIT}"
    )
}

fn seq_scan_heavy_sql() -> String {
    format!(
        "SELECT s.schemaname AS schema_name, s.relname AS table_name, \
         s.seq_scan AS seq_scans, coalesce(s.idx_scan, 0) AS index_scans, \
         round(100.0 * s.seq_scan / nullif(s.seq_scan + coalesce(s.idx_scan, 0), 0), 1) AS seq_scan_percent, \
         s.seq_tup_read AS rows_read_by_seq_scans, s.n_live_tup AS live_rows, \
         pg_catalog.pg_size_pretty(pg_catalog.pg_total_relation_size(s.relid)) AS total_size \
         FROM pg_catalog.pg_stat_user_tables s \
         WHERE s.n_live_tup >= 10000 AND s.seq_scan > coalesce(s.idx_scan, 0) AND {} \
         ORDER BY s.seq_tup_read DESC, s.schemaname, s.relname LIMIT {HEALTH_ROW_LIMIT}",
        visible_stat_table("s")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_report_is_bounded_and_takes_only_the_hidden_schema_parameter() {
        for report in HealthReport::ALL {
            let sql = report.sql();
            assert!(sql.contains(&format!("LIMIT {HEALTH_ROW_LIMIT}")), "{report:?}");
            assert!(sql.contains("$1::text::jsonb"), "{report:?}");
            assert!(!sql.contains("$2"), "{report:?}");
            assert!(!sql.contains(';'), "{report:?}");
        }
    }

    #[test]
    fn reports_use_only_built_in_catalogs_and_no_write_keywords() {
        for report in HealthReport::ALL {
            let upper = report.sql().to_ascii_uppercase();
            let outside_literals: String = upper.split('\'').step_by(2).collect::<Vec<_>>().join(" ");
            let words: Vec<&str> = outside_literals
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .collect();
            for word in [
                "INSERT",
                "UPDATE",
                "DELETE",
                "DROP",
                "ALTER",
                "CREATE",
                "VACUUM",
                "REINDEX",
                "PG_STAT_STATEMENTS",
                "PGSTATTUPLE",
            ] {
                assert!(!words.contains(&word), "{report:?} contains {word}");
            }
            assert!(
                upper.contains("PG_STAT_USER_") || upper.contains("PG_INDEX"),
                "{report:?}"
            );
        }
    }

    #[test]
    fn every_report_checks_what_the_role_can_see() {
        for report in HealthReport::ALL {
            let sql = report.sql();
            assert!(sql.contains("has_schema_privilege"), "{report:?}");
            assert!(sql.contains("has_any_column_privilege"), "{report:?}");
        }
    }

    #[test]
    fn hidden_schemas_travel_as_one_json_text_parameter() {
        assert_eq!(hidden_schemas_param(&[]), Value::Text("[]".into()));
        let value = hidden_schemas_param(&["a\"b".into(), "c".into()]);
        assert_eq!(value, Value::Text("[\"a\\\"b\",\"c\"]".into()));
    }

    #[test]
    fn diagnostics_are_quoted_statements_for_review() {
        let vacuum = HealthReport::TableVacuum.diagnostic_sql("public", "ord\"ers").unwrap();
        assert_eq!(vacuum, "VACUUM (ANALYZE) \"public\".\"ord\"\"ers\"");
        let drop = HealthReport::UnusedIndexes.diagnostic_sql("public", "idx").unwrap();
        assert_eq!(drop, "DROP INDEX CONCURRENTLY IF EXISTS \"public\".\"idx\"");
        assert_eq!(
            HealthReport::DuplicateIndexes.diagnostic_sql("s", "i").unwrap(),
            "DROP INDEX CONCURRENTLY IF EXISTS \"s\".\"i\""
        );
        assert!(
            HealthReport::EstimatedBloat
                .diagnostic_sql("s", "t")
                .unwrap()
                .starts_with("VACUUM")
        );
        assert_eq!(HealthReport::SeqScanHeavy.diagnostic_sql("s", "t"), None);
    }
}
