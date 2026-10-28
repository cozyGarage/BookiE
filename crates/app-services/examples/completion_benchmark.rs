//! Host timing for one completion lookup. Prints one JSON array; run it several times and compare medians.
//!   cargo run --release -p tablepro-app-services --example completion_benchmark
use std::error::Error;
use std::io::Write;
use std::time::Instant;

use tablepro_app_services::completion::SchemaIndex;
use tablepro_app_services::completion_proposals::candidates;
use tablepro_core::sql_syntax::SqlGrammar;

const CATALOG_SIZES: [usize; 3] = [1_000, 10_000, 100_000];
const SCRIPT_STATEMENTS: [usize; 3] = [100, 2_000, 20_000];
const CATALOG_ITERATIONS: usize = 40;
const SCRIPT_ITERATIONS: usize = 7;

struct Timing {
    min_ms: f64,
    median_ms: f64,
    mean_ms: f64,
    matches: usize,
}

fn time_lookups(iterations: usize, mut lookup: impl FnMut() -> usize) -> Timing {
    let mut matches = std::hint::black_box(lookup());
    let mut samples: Vec<f64> = (0..iterations)
        .map(|_| {
            let started = Instant::now();
            matches = std::hint::black_box(lookup());
            started.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    Timing {
        min_ms: samples[0],
        median_ms: samples[samples.len() / 2],
        mean_ms: samples.iter().sum::<f64>() / samples.len() as f64,
        matches,
    }
}

fn catalog(tables: usize) -> SchemaIndex {
    let mut index = SchemaIndex::default();
    index.grammar = Some(SqlGrammar::PostgreSql);
    index.set_tables(
        (0..tables)
            .map(|i| format!("tenant_{:03}.table_{i:06}", i % 200))
            .collect(),
    );
    index
}

fn script(statements: usize) -> String {
    let mut sql = String::new();
    for i in 0..statements {
        sql.push_str(&format!(
            "-- row {i}\nINSERT INTO t{} (id, note, body) VALUES ({i}, 'it''s {i}; not a split', $$dollar; quoted {i}$$);\n/* block; comment {i} */\nSELECT a, b FROM \"t;{}\" WHERE x = {i};\n",
            i % 50,
            i % 7
        ));
    }
    sql.push_str("SELECT * FROM tenant_007.table_0000");
    sql
}

fn row(axis: &str, detail: &str, timing: &Timing) -> String {
    format!(
        r#"{{"axis":"{axis}",{detail},"matches":{},"min_ms":{:.4},"median_ms":{:.4},"mean_ms":{:.4}}}"#,
        timing.matches, timing.min_ms, timing.median_ms, timing.mean_ms
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut rows = Vec::new();
    for tables in CATALOG_SIZES {
        let index = catalog(tables);
        for (prefix, sql) in [
            ("selective", "SELECT * FROM tenant_007.table_0000"),
            ("no_match", "SELECT * FROM zzz"),
        ] {
            let timing = time_lookups(CATALOG_ITERATIONS, || candidates(sql, sql.len(), &index).values.len());
            rows.push(row(
                "catalog",
                &format!(r#""tables":{tables},"prefix":"{prefix}""#),
                &timing,
            ));
        }
    }
    let index = catalog(50);
    for statements in SCRIPT_STATEMENTS {
        let sql = script(statements);
        let timing = time_lookups(SCRIPT_ITERATIONS, || candidates(&sql, sql.len(), &index).values.len());
        rows.push(row(
            "script",
            &format!(r#""script_bytes":{},"tables":50"#, sql.len()),
            &timing,
        ));
    }
    writeln!(std::io::stdout(), "[{}]", rows.join(",\n "))?;
    Ok(())
}
