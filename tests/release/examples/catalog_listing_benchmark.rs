use std::time::{Duration, Instant};

use bookie_core::{Connection, DriverError, OperationControl};
use bookie_release_tests::{Fixture, fixture_enabled};
use uuid::Uuid;

type BoxError = Box<dyn std::error::Error>;

fn peak_rss_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn table_count(argument: Option<String>) -> Result<usize, BoxError> {
    let count = argument.unwrap_or_else(|| "1000".into()).parse::<usize>()?;
    if !(1..=10000).contains(&count) {
        return Err("table count must be between 1 and 10000".into());
    }
    Ok(count)
}

fn setup_sql(prefix: &str, schemas: usize, count: usize) -> String {
    format!(
        "DO $profile$ DECLARE schema_index integer; table_index integer; table_count integer := 0; schema_name text; BEGIN \
         FOR schema_index IN 1..{schemas} LOOP \
         schema_name := format('{prefix}_s%s', schema_index); \
         EXECUTE format('CREATE SCHEMA %I', schema_name); \
         FOR table_index IN 1..100 LOOP \
         EXIT WHEN table_count >= {count}; \
         EXECUTE format('CREATE TABLE %I.%I (id integer)', schema_name, format('t%s', table_index)); \
         table_count := table_count + 1; \
         END LOOP; END LOOP; END $profile$"
    )
}

fn drop_sql(prefix: &str, schemas: usize) -> String {
    let names = (1..=schemas)
        .map(|index| format!("{prefix}_s{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("DROP SCHEMA IF EXISTS {names} CASCADE")
}

async fn setup(
    connection: &dyn Connection,
    prefix: &str,
    count: usize,
    schemas: usize,
    control: &OperationControl,
) -> Result<(), DriverError> {
    connection
        .execute_controlled(&setup_sql(prefix, schemas, count), control)
        .await?;
    Ok(())
}

async fn measure(
    connection: &dyn Connection,
    prefix: &str,
    count: usize,
    control: &OperationControl,
) -> Result<(), BoxError> {
    for attempt in 0..8 {
        let start = Instant::now();
        let tables = connection.list_tables_controlled(control).await?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let profiled = tables
            .iter()
            .filter(|table| table.schema.as_deref().is_some_and(|schema| schema.starts_with(prefix)))
            .count();
        if profiled != count {
            return Err(std::io::Error::other(format!("expected {count} profiled tables, found {profiled}")).into());
        }
        println!(
            "{{\"scenario\":\"list_tables\",\"attempt\":{},\"warmup\":{},\"created_tables\":{},\"listed_tables\":{},\"elapsed_ms\":{:.4},\"peak_rss_kib\":{}}}",
            attempt,
            attempt == 0,
            count,
            tables.len(),
            elapsed_ms,
            peak_rss_kib().map_or("null".into(), |value| value.to_string())
        );
    }
    Ok(())
}

async fn run() -> Result<(), BoxError> {
    if !fixture_enabled() {
        return Err("set BOOKIE_FIXTURE_POSTGRES_RELEASE=1 for a disposable database".into());
    }
    let count = table_count(std::env::args().nth(1))?;
    let schemas = count.div_ceil(100);
    let prefix = format!("bookie_catalog_{}", Uuid::new_v4().simple());
    let fixture = Fixture::from_env();
    let connection = fixture.connect_verified().await;
    let control = OperationControl::with_timeout(Duration::from_secs(120));
    setup(connection.as_ref(), &prefix, count, schemas, &control).await?;
    let result = measure(connection.as_ref(), &prefix, count, &control).await;
    let cleanup = connection
        .execute_controlled(&drop_sql(&prefix, schemas), &control)
        .await;
    result?;
    cleanup?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    run().await
}
