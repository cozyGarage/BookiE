use super::*;
use bookie_core::{ColumnInfo, QueryResult, Value};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::str::FromStr;
use std::sync::Arc;

#[tokio::test]
async fn cancelled_page_load_keeps_its_reservation_until_the_blocking_worker_finishes() {
    use std::future::Future;
    use std::task::Poll;

    let spool = StreamResultSpoolManager::default().create().unwrap();
    let budget = spool.page_budget();
    let lease = budget.reserve(64).unwrap();
    let spool_inner = Arc::clone(&spool.inner);
    let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let blocker = std::thread::spawn(move || {
        let _file = spool_inner.file.lock().unwrap();
        let _ = locked_tx.send(());
        let _ = release_rx.recv();
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), locked_rx)
        .await
        .unwrap()
        .unwrap();
    let mut load = Box::pin(spool.load_page_retained(0, Vec::new(), lease));
    std::future::poll_fn(|context| {
        assert!(load.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !spool.inner.load_worker_started.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(load);
    assert_eq!(budget.used_bytes(), 64);
    let _ = release_tx.send(());
    blocker.join().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while budget.used_bytes() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

struct CountingSequence {
    items: usize,
    serialized: Arc<std::sync::atomic::AtomicUsize>,
}

impl Serialize for CountingSequence {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.items))?;
        for item in 0..self.items {
            sequence.serialize_element(&(item as u64))?;
            self.serialized.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        sequence.end()
    }
}

fn result(values: Vec<Value>) -> Arc<QueryResult> {
    let columns = values
        .iter()
        .enumerate()
        .map(|(index, _)| ColumnInfo {
            name: format!("value_{index}"),
            data_type: "test".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        })
        .collect();
    Arc::new(QueryResult {
        columns,
        rows: vec![values].into(),
        truncated: false,
    })
}

fn typed_values() -> Vec<Value> {
    vec![
        Value::Null,
        Value::Bool(true),
        Value::Int(i64::MIN + 10),
        Value::Float(-0.0),
        Value::Float(f64::from_bits(0x7ff8_0000_0000_0042)),
        Value::Text("typed result".into()),
        Value::Bytes(vec![0, 1, 127, 255]),
        Value::Date(NaiveDate::from_ymd_opt(2026, 10, 25).expect("date")),
        Value::Time(NaiveTime::from_hms_nano_opt(23, 59, 58, 123_456_789).expect("time")),
        Value::DateTime(
            NaiveDateTime::parse_from_str("2026-10-25 23:59:58.123456", "%Y-%m-%d %H:%M:%S%.f").expect("datetime"),
        ),
        Value::TimestampTz(
            DateTime::parse_from_rfc3339("2026-10-25T23:59:58.123456+02:00")
                .expect("timestamp")
                .with_timezone(&Utc),
        ),
        Value::Decimal(Decimal::from_str("12345678901234567890.000100").expect("decimal")),
        Value::Uuid(uuid::Uuid::parse_str("d2ec57c4-4617-4c0f-a312-832cc8c4aef1").expect("uuid")),
        Value::Json(Box::new(serde_json::json!({"exact": [1, true, null]}))),
        Value::Json(Box::new(
            serde_json::from_str("{\"large\":123456789012345678901234567890.123}").expect("large JSON number"),
        )),
        Value::JsonText("{\"raw\":true}".into()),
        Value::Undecodable("vendor_custom_type".into()),
    ]
}

fn assert_values_equal(actual: &[Value], expected: &[Value]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        match (actual, expected) {
            (Value::Float(actual), Value::Float(expected)) => {
                assert_eq!(actual.to_bits(), expected.to_bits());
            }
            (actual, expected) => assert_eq!(actual, expected),
        }
    }
}

#[tokio::test]
async fn round_trips_every_typed_value_without_changing_float_bits() {
    let manager = StreamResultSpoolManager::with_limit(1024 * 1024);
    let spool = manager.create().expect("spool");
    let page = result(typed_values());
    let expected = page.rows[0].to_vec();
    let columns = page.columns.clone();

    spool.append_page(0, page).await.expect("append");
    let loaded = spool.load_page(0, columns).await.expect("load");

    assert_values_equal(&loaded.rows[0], &expected);
}

#[tokio::test]
async fn repeated_page_loads_are_idempotent_and_do_not_reserve_more_quota() {
    let manager = StreamResultSpoolManager::with_limit(1024 * 1024);
    let spool = manager.create().expect("spool");
    let page = result(vec![Value::Text("stable page".into())]);
    let columns = page.columns.clone();
    spool.append_page(0, page).await.expect("append");
    let reserved = manager.reserved_bytes();

    let first = spool.load_page(0, columns.clone()).await.expect("first load");
    let second = spool.load_page(0, columns).await.expect("second load");

    assert_eq!(first.rows, second.rows);
    assert_eq!(manager.reserved_bytes(), reserved);
}

#[tokio::test]
async fn quota_refusal_leaves_no_partial_page_and_releases_reservation() {
    let manager = StreamResultSpoolManager::with_limit(32);
    let spool = manager.create().expect("spool");
    let page = result(vec![Value::Text("larger than quota".into())]);

    let error = spool.append_page(0, page).await.expect_err("quota refusal");

    assert_eq!(error, StreamResultSpoolError::QuotaExceeded);
    assert_eq!(spool.page_count(), 0);
    assert_eq!(manager.reserved_bytes(), 0);
}

#[test]
fn quota_refusal_stops_encoding_before_the_whole_page_is_serialized() {
    let limit = ENCRYPTION_CHUNK_BYTES as u64 + FRAME_HEADER_BYTES + AUTH_TAG_BYTES + 1024;
    let manager = StreamResultSpoolManager::with_limit(limit);
    let spool = manager.create().expect("spool");
    let serialized = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let page = CountingSequence {
        items: 250_000,
        serialized: serialized.clone(),
    };

    let error = append_serialized_page(
        &spool.inner,
        &mut spool.inner.file.lock().expect("spool file"),
        0,
        &page,
    )
    .expect_err("quota refusal");

    assert_eq!(error, StreamResultSpoolError::QuotaExceeded);
    assert!(serialized.load(std::sync::atomic::Ordering::Relaxed) < page.items);
    assert_eq!(spool.page_count(), 0);
    assert_eq!(manager.reserved_bytes(), 0);
}

#[tokio::test]
async fn quota_is_shared_by_all_live_spools_from_the_manager() {
    let manager = StreamResultSpoolManager::with_limit(1024);
    let first = manager.create().expect("first spool");
    let second = manager.create().expect("second spool");
    first
        .append_page(0, result(vec![Value::Int(1)]))
        .await
        .expect("first page");

    let error = second
        .append_page(0, result(vec![Value::Text("x".repeat(1024))]))
        .await
        .expect_err("shared quota refusal");

    assert_eq!(error, StreamResultSpoolError::QuotaExceeded);
    assert_eq!(first.page_count(), 1);
    assert_eq!(second.page_count(), 0);
    assert!(manager.reserved_bytes() > 0);
}

#[tokio::test]
async fn pages_append_only_in_sequence_and_old_pages_remain_readable() {
    let manager = StreamResultSpoolManager::with_limit(1024 * 1024);
    let spool = manager.create().expect("spool");
    let first = result(vec![Value::Int(1)]);
    let columns = first.columns.clone();
    spool.append_page(0, first).await.expect("first page");

    let error = spool
        .append_page(2, result(vec![Value::Int(3)]))
        .await
        .expect_err("out of order");

    assert_eq!(error, StreamResultSpoolError::OutOfOrder);
    assert_eq!(spool.page_count(), 1);
    let second = result(vec![Value::Int(2)]);
    let third = result(vec![Value::Int(3)]);
    spool.append_page(1, second).await.expect("second page");
    spool.append_page(2, third).await.expect("third page");
    let reserved = manager.reserved_bytes();

    for (index, value) in [(0, 1), (1, 2), (2, 3), (0, 1)] {
        assert_eq!(
            spool.load_page(index, columns.clone()).await.expect("load").rows,
            vec![vec![Value::Int(value)]]
        );
    }
    assert_eq!(manager.reserved_bytes(), reserved);
}

#[tokio::test]
async fn page_index_metadata_has_a_hard_bound() {
    let manager = StreamResultSpoolManager::with_limit(1024 * 1024);
    let spool = manager.create().expect("spool");
    for index in 0..MAX_STREAM_RESULT_SPOOL_PAGES {
        spool
            .append_page(index as u64, result(vec![Value::Int(index as i64)]))
            .await
            .expect("append within page limit");
    }

    let error = spool
        .append_page(MAX_STREAM_RESULT_SPOOL_PAGES as u64, result(vec![Value::Int(1)]))
        .await
        .expect_err("page limit");

    assert_eq!(error, StreamResultSpoolError::PageLimit);
    assert_eq!(spool.page_count(), MAX_STREAM_RESULT_SPOOL_PAGES);
}

#[tokio::test]
async fn dropping_the_last_spool_owner_returns_its_process_quota() {
    let manager = StreamResultSpoolManager::with_limit(1024 * 1024);
    let spool = manager.create().expect("spool");
    spool
        .append_page(0, result(vec![Value::Text("owned".into())]))
        .await
        .expect("append");
    let reserved = manager.reserved_bytes();
    assert!(reserved > 0);

    let owner = spool.clone();
    let raw_fd = spool.inner.file.lock().expect("spool file").file.as_raw_fd();
    let spool_path = std::fs::read_link(format!("/proc/self/fd/{raw_fd}")).expect("spool descriptor");
    drop(spool);

    assert_eq!(manager.reserved_bytes(), reserved);

    drop(owner);

    assert_eq!(manager.reserved_bytes(), 0);
    assert_ne!(
        std::fs::read_link(format!("/proc/self/fd/{raw_fd}")).ok(),
        Some(spool_path)
    );
}

#[tokio::test]
async fn tampered_ciphertext_is_refused_instead_of_returning_rows() {
    let manager = StreamResultSpoolManager::with_limit(1024 * 1024);
    let spool = manager.create().expect("spool");
    spool
        .append_page(0, result(vec![Value::Text("private".into())]))
        .await
        .expect("append");
    {
        let mut file = spool.inner.file.lock().expect("spool file");
        file.file.seek(SeekFrom::End(-1)).expect("last byte");
        let mut last = [0_u8; 1];
        file.file.read_exact(&mut last).expect("read last byte");
        last[0] ^= 1;
        file.file.seek(SeekFrom::End(-1)).expect("last byte");
        file.file.write_all(&last).expect("tamper");
    }

    let error = spool
        .load_page(0, result(vec![Value::Null]).columns.clone())
        .await
        .expect_err("authentication failure");

    assert_eq!(error, StreamResultSpoolError::Encryption);
}

#[test]
fn only_an_existing_absolute_directory_is_a_usable_spool_location() {
    let dir = tempfile::tempdir().unwrap();
    assert!(usable_spool_directory(dir.path()));
    assert!(!usable_spool_directory(&dir.path().join("missing")));
    assert!(!usable_spool_directory(Path::new("relative/spool")));
}

#[test]
fn a_spool_in_a_chosen_directory_stores_and_reloads_pages() {
    let dir = tempfile::tempdir().unwrap();
    let manager = StreamResultSpoolManager::default().in_directory(Some(dir.path().to_path_buf()));
    let spool = manager.create().unwrap();
    let page = result(vec![Value::Int(7)]);
    spool.append_page_sync(0, &page).unwrap();
    let loaded = spool.load_page_sync(0, page.columns.clone()).unwrap();
    assert_eq!(loaded.rows[0][0], Value::Int(7));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn a_missing_spool_directory_fails_to_create_instead_of_falling_back() {
    let dir = tempfile::tempdir().unwrap();
    let manager = StreamResultSpoolManager::default().in_directory(Some(dir.path().join("missing")));
    assert_eq!(manager.create().err(), Some(StreamResultSpoolError::Create));
}

#[test]
fn available_bytes_shrink_as_pages_are_stored() {
    let manager = StreamResultSpoolManager::with_limit(1 << 20);
    let spool = manager.create().unwrap();
    assert_eq!(manager.available_bytes(), 1 << 20);
    spool.append_page_sync(0, &result(vec![Value::Int(1)])).unwrap();
    assert_eq!(manager.available_bytes(), (1 << 20) - spool.stored_bytes());
    assert_eq!(manager.limit_bytes(), 1 << 20);
}
