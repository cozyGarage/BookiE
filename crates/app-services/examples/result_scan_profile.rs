use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bookie_app_services::result_compare::{CompareInput, CompareRequest, compare_results};
use bookie_app_services::result_filter::filter_into_spool;
use bookie_app_services::result_find::find_matches;
use bookie_app_services::result_predicate::{ColumnScope, ResultPredicate};
use bookie_app_services::result_scan::{ScanProgress, ScanSource};
use bookie_app_services::stream_result_spool::{StreamResultSpool, StreamResultSpoolManager};
use bookie_core::{ColumnInfo, QueryResult, Value};
use serde_json::{Value as Json, json};
use tokio_util::sync::CancellationToken;

const PAGE_ROWS: usize = 5_000;

type Failure = Box<dyn std::error::Error + Send + Sync>;

fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "profile".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn columns() -> Vec<ColumnInfo> {
    ["id", "label", "payload", "amount", "flag", "note"]
        .iter()
        .map(|name| column(name))
        .collect()
}

fn row(id: i64, payload: &str, label_suffix: &str) -> Vec<Value> {
    vec![
        Value::Int(id),
        Value::Text(format!("item-{id}{label_suffix}")),
        Value::Text(payload.into()),
        Value::Float(id as f64 * 0.25),
        Value::Bool(id % 2 == 0),
        if id % 7 == 0 {
            Value::Null
        } else {
            Value::Text(format!("note {id}"))
        },
    ]
}

async fn build_spool(
    manager: &StreamResultSpoolManager,
    rows: i64,
    payload: &str,
    edit: impl Fn(i64) -> Option<&'static str>,
) -> Result<(StreamResultSpool, u64), Failure> {
    let spool = manager.create()?;
    let mut index = 0_u64;
    let mut total = 0_u64;
    let mut page: Vec<Vec<Value>> = Vec::new();
    for id in 0..rows {
        match edit(id) {
            Some("drop") => continue,
            Some(suffix) => page.push(row(id, payload, suffix)),
            None => page.push(row(id, payload, "")),
        }
        if page.len() == PAGE_ROWS {
            total += flush(&spool, &mut page, index).await?;
            index += 1;
        }
    }
    if !page.is_empty() {
        total += flush(&spool, &mut page, index).await?;
    }
    Ok((spool, total))
}

async fn flush(spool: &StreamResultSpool, page: &mut Vec<Vec<Value>>, index: u64) -> Result<u64, Failure> {
    let rows = std::mem::take(page);
    let count = rows.len() as u64;
    let result = QueryResult {
        columns: columns(),
        rows: rows.into(),
        truncated: false,
    };
    spool.append_page(index, Arc::new(result)).await?;
    Ok(count)
}

fn status_kib(key: &str) -> u64 {
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    text.lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn pss_kib() -> u64 {
    let text = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    text.lines()
        .find_map(|line| line.strip_prefix("Pss:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn reset_peak() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

fn latency_json(mut millis: Vec<f64>) -> Json {
    millis.sort_by(f64::total_cmp);
    json!({
        "samples": millis.len(),
        "p50_ms": percentile(&millis, 0.5),
        "p95_ms": percentile(&millis, 0.95),
        "max_ms": millis.last().copied().unwrap_or(0.0),
    })
}

async fn probe_latency(spool: StreamResultSpool, stop: Arc<AtomicBool>, pages: usize) -> Result<Vec<f64>, Failure> {
    let mut samples = Vec::new();
    let mut index = 0_usize;
    while !stop.load(Ordering::Acquire) {
        index = (index * 31 + 17) % pages.max(1);
        let started = Instant::now();
        spool.load_page(index as u64, columns()).await?;
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(samples)
}

async fn idle_latency(spool: &StreamResultSpool, pages: usize) -> Result<Json, Failure> {
    let stop = Arc::new(AtomicBool::new(false));
    let probe = tokio::spawn(probe_latency(spool.clone(), stop.clone(), pages));
    tokio::time::sleep(Duration::from_secs(2)).await;
    stop.store(true, Ordering::Release);
    Ok(latency_json(probe.await??))
}

async fn measured<T: Send + 'static>(
    spool: &StreamResultSpool,
    pages: usize,
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<(T, Json), Failure> {
    reset_peak();
    let rss_before = status_kib("VmRSS:");
    let stop = Arc::new(AtomicBool::new(false));
    let probe = tokio::spawn(probe_latency(spool.clone(), stop.clone(), pages));
    let started = Instant::now();
    let output = tokio::task::spawn_blocking(work).await??;
    let elapsed = started.elapsed().as_secs_f64();
    stop.store(true, Ordering::Release);
    let latency = latency_json(probe.await??);
    let stats = json!({
        "seconds": elapsed,
        "rss_before_kib": rss_before,
        "rss_after_kib": status_kib("VmRSS:"),
        "rss_peak_kib": status_kib("VmHWM:"),
        "pss_after_kib": pss_kib(),
        "page_load_latency_during": latency,
    });
    Ok((output, stats))
}

fn emit(label: &str, body: Json) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "PROFILE {}", json!({ label: body }));
}

fn env_number(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let rows = env_number("BOOKIE_PROFILE_ROWS", 500_000);
    let payload = "x".repeat(env_number("BOOKIE_PROFILE_PAYLOAD_BYTES", 200) as usize);
    let manager = StreamResultSpoolManager::with_limit(64 << 30)
        .in_directory(bookie_app_services::stream_result_spool::spool_directory_from_environment());
    let base_rss = status_kib("VmRSS:");
    let (left_spool, left_rows) = build_spool(&manager, rows, &payload, |_| None).await?;
    let pages = left_spool.page_count();
    emit(
        "setup",
        json!({
            "rows": left_rows,
            "pages": pages,
            "spool_bytes": left_spool.stored_bytes(),
            "rss_before_kib": base_rss,
            "rss_after_kib": status_kib("VmRSS:"),
            "spool_dir": std::env::var("BOOKIE_SPOOL_DIR").unwrap_or_else(|_| "default temp dir".into()),
            "idle_latency": idle_latency(&left_spool, pages).await?,
        }),
    );
    let left = ScanSource::new(left_spool.clone(), columns(), left_rows);
    profile_find_and_filter(&manager, &left, &left_spool, pages).await?;
    profile_compare(&manager, &left, left_rows, rows, &payload, pages).await
}

async fn profile_find_and_filter(
    manager: &StreamResultSpoolManager,
    left: &ScanSource,
    spool: &StreamResultSpool,
    pages: usize,
) -> Result<(), Failure> {
    let source = left.clone();
    let (report, stats) = measured(spool, pages, move || {
        let predicate = ResultPredicate::Contains {
            scope: ColumnScope::Any,
            needle: "zzz-not-present".into(),
            options: Default::default(),
        };
        Ok(find_matches(
            &source,
            predicate,
            &CancellationToken::new(),
            &ScanProgress::default(),
        )?)
    })
    .await?;
    emit(
        "find_full_scan",
        json!({ "rows_scanned": report.rows_scanned, "stats": stats }),
    );
    let source = left.clone();
    let manager = manager.clone();
    let (filtered, stats) = measured(spool, pages, move || {
        let predicate = ResultPredicate::Contains {
            scope: ColumnScope::Column(1),
            needle: "item-7".into(),
            options: Default::default(),
        };
        Ok(filter_into_spool(
            &source,
            predicate,
            &manager,
            &CancellationToken::new(),
            &ScanProgress::default(),
        )?)
    })
    .await?;
    emit(
        "filter_to_derived_spool",
        json!({
            "rows_kept": filtered.total_rows,
            "derived_pages": filtered.page_row_counts.len(),
            "derived_spool_bytes": filtered.spool.stored_bytes(),
            "stats": stats,
        }),
    );
    Ok(())
}

async fn profile_compare(
    manager: &StreamResultSpoolManager,
    left: &ScanSource,
    left_rows: u64,
    rows: i64,
    payload: &str,
    pages: usize,
) -> Result<(), Failure> {
    let edit = |id: i64| match id % 100 {
        1 => Some("-edited"),
        2 => Some("drop"),
        _ => None,
    };
    let (right_spool, right_rows) = build_spool(manager, rows, payload, edit).await?;
    let right = ScanSource::new(right_spool, columns(), right_rows);
    for keys in [Vec::new(), vec!["id".to_string()]] {
        let request = CompareRequest {
            left: CompareInput::Spool(left.clone()),
            right: CompareInput::Spool(right.clone()),
            key_columns: keys.clone(),
        };
        let (report, stats) = measured(left.spool(), pages, move || {
            Ok(compare_results(
                &request,
                &CancellationToken::new(),
                &ScanProgress::default(),
            )?)
        })
        .await?;
        emit(
            if keys.is_empty() {
                "compare_whole_row"
            } else {
                "compare_keyed"
            },
            json!({
                "rows_left": left_rows,
                "rows_right": right_rows,
                "added": report.summary.added,
                "removed": report.summary.removed,
                "changed": report.summary.changed,
                "equal": report.summary.equal,
                "diff_rows": report.diff.rows.len(),
                "stats": stats,
            }),
        );
    }
    Ok(())
}
