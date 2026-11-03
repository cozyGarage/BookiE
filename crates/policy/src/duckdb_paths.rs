use std::ops::ControlFlow;

use sqlparser::ast::{Ident, ObjectName, ObjectNamePart, Statement, TableFactor, Visit, Visitor};

const SAFE_TABLE_FUNCTIONS: &[&str] = &[
    "range",
    "generate_series",
    "generate_subscripts",
    "unnest",
    "json_each",
    "json_tree",
    "repeat_row",
    "duckdb_tables",
    "duckdb_columns",
    "duckdb_views",
    "duckdb_schemas",
    "duckdb_indexes",
    "duckdb_constraints",
    "duckdb_sequences",
    "duckdb_types",
    "duckdb_functions",
    "duckdb_keywords",
    "duckdb_dependencies",
    "duckdb_comments",
    "pragma_table_info",
    "pragma_storage_info",
    "pragma_version",
];

const BUILTIN_SCHEMAS: [&str; 2] = ["main", "system"];

struct SourceFinder;

pub(crate) fn touches_foreign_source(statements: &[Statement]) -> bool {
    statements
        .iter()
        .any(|statement| statement.visit(&mut SourceFinder).is_break())
}

impl Visitor for SourceFinder {
    type Break = ();

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
        match statement {
            Statement::ExplainTable { table_name, .. }
                if names_a_path(table_name) || names_an_unlisted_catalog_source(table_name) =>
            {
                ControlFlow::Break(())
            }
            _ => ControlFlow::Continue(()),
        }
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<()> {
        let foreign = match factor {
            TableFactor::Table {
                name, args: Some(_), ..
            }
            | TableFactor::Function { name, .. } => !is_safe_table_function(name),
            TableFactor::Table { name, args: None, .. } => names_a_path(name) || names_an_unlisted_catalog_source(name),
            TableFactor::Derived { .. }
            | TableFactor::NestedJoin { .. }
            | TableFactor::Pivot { .. }
            | TableFactor::Unpivot { .. }
            | TableFactor::UnpivotExpr { .. }
            | TableFactor::UNNEST { .. }
            | TableFactor::MatchRecognize { .. } => false,
            _ => true,
        };
        if foreign {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

fn identifiers(name: &ObjectName) -> impl Iterator<Item = &Ident> {
    name.0.iter().filter_map(|part| match part {
        ObjectNamePart::Identifier(ident) => Some(ident),
        _ => None,
    })
}

fn is_safe_table_function(name: &ObjectName) -> bool {
    let parts: Vec<&Ident> = identifiers(name).collect();
    let qualifier_is_builtin = match parts.as_slice() {
        [_] => true,
        [schema, _] => BUILTIN_SCHEMAS.contains(&schema.value.to_ascii_lowercase().as_str()),
        _ => false,
    };
    qualifier_is_builtin
        && parts
            .last()
            .is_some_and(|ident| SAFE_TABLE_FUNCTIONS.contains(&ident.value.to_ascii_lowercase().as_str()))
}

fn names_a_path(name: &ObjectName) -> bool {
    identifiers(name).last().is_some_and(|ident| match ident.quote_style {
        Some('\'') => true,
        Some(_) => looks_like_a_path(&ident.value),
        None => false,
    })
}

fn names_an_unlisted_catalog_source(name: &ObjectName) -> bool {
    let parts: Vec<&Ident> = identifiers(name).collect();
    let [.., last] = parts.as_slice() else {
        return false;
    };
    let lowered = last.value.to_ascii_lowercase();
    let catalog_name = lowered.starts_with("duckdb_") || lowered.starts_with("pragma_");
    catalog_name && !is_safe_table_function(name)
}

fn looks_like_a_path(name: &str) -> bool {
    name.contains(['/', '\\', '.', '*', '?'])
}

#[cfg(test)]
mod tests {
    use crate::classify::classify;
    use crate::effects::Effects;

    #[test]
    fn duckdb_file_and_url_readers_are_not_plain_reads() {
        for sql in [
            "SELECT * FROM read_csv('/etc/passwd')",
            "SELECT * FROM read_csv_auto('/etc/passwd')",
            "SELECT * FROM read_parquet('/data/x.parquet')",
            "SELECT * FROM parquet_scan('/data/x.parquet')",
            "SELECT * FROM parquet_metadata('/data/x.parquet')",
            "SELECT * FROM parquet_schema('/data/x.parquet')",
            "SELECT * FROM parquet_file_metadata('/data/x.parquet')",
            "SELECT * FROM parquet_kv_metadata('/data/x.parquet')",
            "SELECT * FROM glob('/home/*/.ssh/*')",
            "SELECT * FROM glob('s3://bucket/*')",
            "SELECT * FROM read_json_objects('/etc/passwd')",
            "SELECT * FROM read_json_objects_auto('/etc/passwd')",
            "SELECT * FROM read_ndjson_objects('/etc/passwd')",
            "SELECT * FROM read_json('/data/x.json')",
            "SELECT * FROM read_ndjson('/data/x.json')",
            "SELECT * FROM sniff_csv('/etc/passwd')",
            "SELECT read_text('/etc/passwd')",
            "SELECT getenv('HOME')",
            "SELECT current_setting('s3_secret_access_key')",
            "SELECT which_secret('s3://bucket/x', 's3')",
            "SELECT read_blob('/etc/passwd')",
            "SELECT * FROM read_xlsx('/data/x.xlsx')",
            "SELECT * FROM st_read('/data/x.geojson')",
            "SELECT * FROM delta_scan('/data/delta')",
            "SELECT * FROM iceberg_scan('/data/iceberg')",
            "SELECT * FROM iceberg_metadata('/data/iceberg')",
            "SELECT * FROM iceberg_snapshots('/data/iceberg')",
            "SELECT * FROM read_json_auto('/data/x.json')",
            "SELECT * FROM read_ndjson_auto('/data/x.json')",
        ] {
            let facts = classify(sql, "duckdb");
            assert!(facts.effects.contains(Effects::HOST_OR_FILE_ACCESS), "{sql}");
            assert!(!facts.is_single_read(), "{sql}");
        }
    }

    #[test]
    fn duckdb_reading_a_file_by_path_is_not_a_plain_read() {
        for sql in [
            "SELECT * FROM '/etc/passwd.csv'",
            "SELECT * FROM '/data/x.parquet'",
            "SELECT * FROM \"/data/x.parquet\"",
            "SELECT * FROM orders o JOIN '/data/x.csv' c ON c.id = o.id",
            "SELECT * FROM 'https://example.com/x.csv'",
            "SELECT * FROM query('SELECT * FROM read_csv(''/etc/passwd'')')",
            "SELECT * FROM query_table('orders')",
            "SELECT * FROM sqlite_scan('/x.db', 't')",
            "SELECT * FROM postgres_scan('host=x', 'public', 't')",
            "SELECT * FROM mysql_scan('host=x', 'db', 't')",
            "SELECT * FROM read_avro('/data/x.avro')",
            "SELECT * FROM main.query('SELECT 1')",
            "SELECT * FROM orders o JOIN LATERAL query('SELECT 1') q ON true",
            "SELECT * FROM 's3://bucket/x.parquet'",
            "SELECT * FROM orders, '/data/x.csv'",
            "SELECT * FROM orders o, (SELECT 1) s, '/data/x.csv' c",
            "SELECT * FROM orders WHERE id IN (1) UNION SELECT * FROM '/data/x.csv'",
            "SELECT * FROM (SELECT 1 FROM t) s, '/data/x.csv'",
            "SELECT * FROM orders o, (SELECT 1 FROM t) s, '/etc/passwd.csv' c",
            "SELECT * FROM \"secrets.csv.gz\"",
            "SELECT * FROM \"dump.csv.zst\"",
            "SELECT * FROM \"x.arrow\"",
            "SELECT * FROM ('/etc/passwd.csv')",
            "DESCRIBE '/etc/x.csv'",
            "SELECT * FROM orders WHERE id IN (SELECT id FROM '/data/x.csv')",
            "DELETE FROM t USING '/etc/x.csv' c WHERE t.id = c.id",
            "DELETE FROM t USING read_avro('/x.avro') c WHERE t.id = c.id",
            "DELETE FROM t USING sqlite_scan('/x.db', 'u') c WHERE t.id = c.id",
            "SELECT * FROM orders WHERE EXISTS (SELECT 1 FROM '/etc/x.csv')",
            "WITH f AS (SELECT * FROM '/etc/x.csv') SELECT * FROM f",
            "INSERT INTO t SELECT * FROM '/etc/x.csv'",
            "SELECT * FROM \"data/x\"",
            "SELECT * FROM \"events-*.parquet\"",
            "SELECT * FROM \"x.tbl\"",
            "SELECT * FROM \"orders.json\"",
            "SELECT * FROM \"public.orders\"",
            "SELECT * FROM duckdb_secrets(redact := false)",
            "SELECT * FROM duckdb_settings()",
            "SELECT * FROM duckdb_databases()",
            "SELECT * FROM duckdb_extensions()",
            "SELECT * FROM duckdb_temporary_files()",
            "SELECT * FROM x.unnest('x')",
            "SELECT * FROM other.range(5)",
            "SELECT * FROM \"duckdb_secrets\"",
            "SELECT * FROM \"main\".\"duckdb_logs\"",
            "SELECT * FROM `duckdb_secrets`",
            "SELECT * FROM \"DUCKDB_SETTINGS\"",
            "DESCRIBE \"duckdb_secrets\"",
            "SELECT * FROM pragma_database_list",
            "SELECT * FROM duckdb_logs",
            "SELECT * FROM duckdb_log_contexts",
            "SELECT * FROM duckdb_variables",
            "SELECT * FROM duckdb_prepared_statements",
            "SELECT * FROM duckdb_external_file_cache",
            "SELECT * FROM duckdb_memory",
            "SELECT * FROM pragma_database_list",
            "DESCRIBE duckdb_secrets",
            "DESCRIBE duckdb_settings",
            "SELECT * FROM duckdb_secrets",
            "SELECT * FROM duckdb_settings",
            "SELECT * FROM main.duckdb_extensions",
            "MERGE INTO t USING '/etc/x.csv' c ON t.id = c.id WHEN MATCHED THEN DELETE",
            "MERGE INTO t USING read_avro('/x.avro') c ON t.id = c.id WHEN MATCHED THEN DELETE",
            "UPDATE t SET v = 1 FROM '/etc/x.csv' c WHERE t.id = c.id",
        ] {
            let facts = classify(sql, "duckdb");
            assert!(facts.effects.contains(Effects::HOST_OR_FILE_ACCESS), "{sql}");
            assert!(!facts.is_single_read(), "{sql}");
        }
    }

    #[test]
    fn duckdb_delete_using_a_table_carries_no_file_access() {
        let facts = classify("DELETE FROM t USING items i WHERE t.id = i.id", "duckdb");
        assert!(!facts.effects.contains(Effects::HOST_OR_FILE_ACCESS));
        let facts = classify("DELETE FROM t USING '/etc/x.csv' c WHERE t.id = c.id", "duckdb");
        assert!(facts.effects.contains(Effects::HOST_OR_FILE_ACCESS));
    }

    #[test]
    fn duckdb_statements_the_parser_rejects_are_not_plain_reads() {
        let facts = classify("SUMMARIZE '/etc/x.csv'", "duckdb");
        assert!(!facts.is_single_read());
    }

    #[test]
    fn duckdb_ordinary_queries_stay_plain_reads() {
        for sql in [
            "SELECT * FROM orders",
            "SELECT count(*) FROM range(10)",
            "SELECT * FROM generate_series(1, 5)",
            "SELECT * FROM duckdb_tables()",
            "SELECT 'a', 'b' FROM orders WHERE name = 'x'",
            "SELECT * FROM orders WHERE note IN ('a', 'b')",
            "SELECT * FROM orders o JOIN items i ON i.kind = 'x' AND i.id = o.id",
            "SELECT concat(a, '/b.csv') FROM orders",
            "SELECT * FROM orders WHERE status IS DISTINCT FROM 'x'",
            "SELECT extract(year FROM '2020-01-01'::date)",
            "SELECT trim(BOTH 'x' FROM col) FROM orders",
            "SELECT substring(name FROM 'a.*') FROM orders",
            "SELECT * FROM orders WHERE id IN (SELECT id FROM items)",
            "SELECT * FROM generate_series(1, 5) g, range(3) r",
            "SELECT * FROM unnest([1, 2, 3])",
            "SELECT * FROM pragma_table_info('orders')",
            "SELECT * FROM main.range(5)",
            "SELECT * FROM duckdb_columns()",
            "SELECT * FROM duckdb_tables",
            "SELECT * FROM \"duckdb_columns\"",
            "SELECT * FROM pragma_table_info('orders')",
            "DESCRIBE duckdb_tables",
            "SELECT * FROM analytics.orders",
            "DESCRIBE orders",
            "SELECT * FROM \"my.db\".main.orders",
            "SELECT * FROM orders PIVOT (sum(v) FOR k IN ('a', 'b'))",
            "SELECT * FROM duckdb_functions()",
            "SELECT * FROM (VALUES (1, 'a')) AS t(a, b)",
            "SELECT * FROM orders o JOIN (VALUES (1, 'a')) v(a, b) ON v.a = o.id",
            "SELECT * FROM orders o JOIN items i ON i.x = ['a', 'b']",
            "SELECT * FROM orders o JOIN items i ON i.s = {'a': 'x', 'b': 'y'}",
            "SELECT * FROM orders o JOIN items i USING (id)",
            "SELECT * FROM orders WHERE path = '/data/x.csv'",
        ] {
            let facts = classify(sql, "duckdb");
            assert!(!facts.effects.contains(Effects::HOST_OR_FILE_ACCESS), "{sql}");
            assert!(facts.is_single_read(), "{sql}");
        }
    }
}
