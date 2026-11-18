use std::time::SystemTime;

pub use bookie_storage::query_history::Source;
use bookie_storage::query_history::{Entry, HistoryStore, NewEntry, Outcome, SearchFilter};
use uuid::Uuid;

pub struct Statement {
    pub query: String,
    pub driver_id: String,
    pub connection_id: Uuid,
    pub connection_name: String,
    pub started_at: SystemTime,
    pub duration_ms: i64,
    pub rows: Option<i64>,
    pub error: Option<String>,
    pub cancelled: bool,
    pub source: Source,
}

pub struct RecentStatement {
    pub query: String,
    pub connection_name: String,
    pub executed_at: SystemTime,
    pub duration_ms: Option<i64>,
    pub succeeded: bool,
}

pub async fn record(statement: Statement) -> Result<(), String> {
    let store = HistoryStore::open_default().await.map_err(|error| error.to_string())?;
    record_in(&store, statement).await
}

pub async fn recent(limit: usize) -> Result<Vec<RecentStatement>, String> {
    let store = HistoryStore::open_default().await.map_err(|error| error.to_string())?;
    recent_in(&store, limit).await
}

pub async fn record_in(store: &HistoryStore, statement: Statement) -> Result<(), String> {
    let outcome = match (statement.cancelled, statement.error) {
        (true, _) => Outcome::Cancelled,
        (false, Some(message)) => Outcome::Error(message),
        (false, None) => Outcome::Success,
    };
    store
        .record(NewEntry {
            query: statement.query,
            driver_id: statement.driver_id,
            connection_id: statement.connection_id,
            connection_name: statement.connection_name,
            executed_at: statement.started_at,
            duration_ms: Some(statement.duration_ms),
            rows_affected: statement.rows,
            outcome,
            source: statement.source,
        })
        .await
        .map(drop)
        .map_err(|error| error.to_string())
}

pub(crate) async fn recent_in(store: &HistoryStore, limit: usize) -> Result<Vec<RecentStatement>, String> {
    let entries = store
        .search(SearchFilter {
            limit,
            ..SearchFilter::default()
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(entries.into_iter().map(recent_statement).collect())
}

fn recent_statement(entry: Entry) -> RecentStatement {
    RecentStatement {
        query: entry.query,
        connection_name: entry.connection_name,
        executed_at: entry.executed_at,
        duration_ms: entry.duration_ms,
        succeeded: entry.success && !entry.cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statement(query: &str, error: Option<&str>) -> Statement {
        Statement {
            query: query.into(),
            driver_id: "sqlite".into(),
            connection_id: Uuid::new_v4(),
            connection_name: "demo".into(),
            started_at: SystemTime::now(),
            duration_ms: 3,
            rows: Some(2),
            error: error.map(str::to_owned),
            cancelled: false,
            source: Source::Editor,
        }
    }

    #[tokio::test]
    async fn a_recorded_statement_comes_back_newest_first_with_its_outcome() {
        let path = std::env::temp_dir().join(format!("bookie-history-{}.db", Uuid::new_v4()));
        let store = HistoryStore::open(path.clone()).await.unwrap();
        record_in(&store, statement("SELECT 1", None)).await.unwrap();
        record_in(&store, statement("SELECT nope", Some("no such column")))
            .await
            .unwrap();
        let recent = recent_in(&store, 10).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().any(|row| row.query == "SELECT 1" && row.succeeded));
        assert!(recent.iter().any(|row| row.query == "SELECT nope" && !row.succeeded));
        assert!(recent.iter().all(|row| row.connection_name == "demo"));
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
