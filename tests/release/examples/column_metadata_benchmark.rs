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

fn profile_args() -> Result<(usize, bool), BoxError> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let width = args.first().cloned().unwrap_or_else(|| "64".into()).parse::<usize>()?;
    if !(1..=1500).contains(&width) {
        return Err("column count must be between 1 and 1500".into());
    }
    let rich = args.get(1).is_some_and(|value| value == "rich");
    if args.get(1).is_some_and(|value| value != "rich" && value != "simple") {
        return Err("profile kind must be simple or rich".into());
    }
    Ok((width, rich))
}

fn table_sql(schema: &str, table: &str, width: usize, rich: bool) -> String {
    let mut columns = vec!["id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY".to_string()];
    columns.extend((1..width).map(|index| {
        let kind = if !rich {
            "integer".to_string()
        } else {
            match index % 5 {
                0 => format!("{schema}.state"),
                1 => format!("{schema}.code"),
                2 => "numeric(18,4)".into(),
                3 => "timestamp with time zone".into(),
                _ => "text".into(),
            }
        };
        format!("c{index} {kind}")
    }));
    format!("CREATE TABLE {schema}.{table} ({})", columns.join(", "))
}

async fn setup(
    connection: &dyn Connection,
    schema: &str,
    table: &str,
    width: usize,
    rich: bool,
    control: &OperationControl,
) -> Result<(), DriverError> {
    connection
        .execute_controlled(&format!("CREATE SCHEMA {schema}"), control)
        .await?;
    let result = async {
        if rich {
            connection
                .execute_controlled(
                    &format!("CREATE TYPE {schema}.state AS ENUM ('fresh', 'done')"),
                    control,
                )
                .await?;
            connection
                .execute_controlled(&format!("CREATE DOMAIN {schema}.code AS varchar(32) NOT NULL"), control)
                .await?;
        }
        connection
            .execute_controlled(&table_sql(schema, table, width, rich), control)
            .await?;
        Ok::<(), DriverError>(())
    }
    .await;
    if result.is_err() {
        let _ = connection
            .execute_controlled(&format!("DROP SCHEMA {schema} CASCADE"), control)
            .await;
    }
    result
}

async fn measure(
    connection: &dyn Connection,
    schema: &str,
    table: &str,
    width: usize,
    rich: bool,
    control: &OperationControl,
) -> Result<(), BoxError> {
    for attempt in 0..8 {
        let start = Instant::now();
        let columns = connection
            .fetch_columns_controlled(Some(schema), table, control)
            .await?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        if columns.len() != width {
            return Err(std::io::Error::other(format!("expected {width} columns, found {}", columns.len())).into());
        }
        println!(
            "{{\"kind\":\"{}\",\"width\":{},\"attempt\":{},\"warmup\":{},\"elapsed_ms\":{:.4},\"columns\":{},\"peak_rss_kib\":{}}}",
            if rich { "rich" } else { "simple" },
            width,
            attempt,
            attempt == 0,
            elapsed_ms,
            columns.len(),
            peak_rss_kib().map_or("null".into(), |value| value.to_string())
        );
    }
    Ok(())
}

async fn run() -> Result<(), BoxError> {
    if !fixture_enabled() {
        return Err("set BOOKIE_FIXTURE_POSTGRES_RELEASE=1 for a disposable database".into());
    }
    let (width, rich) = profile_args()?;
    let fixture = Fixture::from_env();
    let connection = fixture.connect_verified().await;
    let schema = format!("bookie_meta_{}", Uuid::new_v4().simple());
    let table = "metadata_rows";
    let control = OperationControl::with_timeout(Duration::from_secs(120));
    setup(connection.as_ref(), &schema, table, width, rich, &control).await?;
    let result = measure(connection.as_ref(), &schema, table, width, rich, &control).await;
    let cleanup = connection
        .execute_controlled(&format!("DROP SCHEMA {schema} CASCADE"), &control)
        .await;
    result?;
    cleanup?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    run().await
}
