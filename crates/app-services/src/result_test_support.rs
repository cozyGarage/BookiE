use bookie_core::{ColumnInfo, QueryResult, Value};

use crate::result_scan::ScanSource;
use crate::stream_result_spool::StreamResultSpoolManager;

pub(crate) fn column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
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
    }
}

pub(crate) fn page(rows: Vec<Vec<Value>>) -> QueryResult {
    QueryResult {
        columns: vec![column("id"), column("label")],
        rows: rows.into(),
        truncated: false,
    }
}

pub(crate) fn source_of(pages: Vec<Vec<Vec<Value>>>) -> ScanSource {
    let spool = StreamResultSpoolManager::default().create().unwrap();
    let mut total = 0_u64;
    for (index, rows) in pages.into_iter().enumerate() {
        total += rows.len() as u64;
        spool.append_page_sync(index as u64, &page(rows)).unwrap();
    }
    ScanSource::new(spool, vec![column("id"), column("label")], total)
}

pub(crate) fn numbered(start: i64, count: i64) -> Vec<Vec<Value>> {
    (start..start + count)
        .map(|id| vec![Value::Int(id), Value::Text(format!("row {id}"))])
        .collect()
}
