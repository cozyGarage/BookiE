use std::sync::{Arc, Mutex};

use bookie_core::{ConnectOptions, DatabaseDriver, Value};
use drivers_sqlite::SqliteDriver;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, filter::LevelFilter, layer::Context, layer::SubscriberExt};

#[tokio::test]
async fn a_simple_select_uses_one_embedded_sql_statement() {
    let connection = SqliteDriver
        .connect(ConnectOptions {
            database: ":memory:".into(),
            ..Default::default()
        })
        .await
        .expect("open in-memory SQLite");
    let statements = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry()
        .with(QueryStatements(Arc::clone(&statements)))
        .with(LevelFilter::TRACE);
    tracing::subscriber::set_global_default(subscriber).expect("install SQL trace subscriber");
    let query = "SELECT 42 AS value /* bookie_round_trip_budget_sqlite */";
    let result = connection.query(query).await.expect("run measured SELECT");
    assert_eq!(result.rows, vec![vec![Value::Int(42)]]);

    let statements = statements.lock().expect("SQL trace lock");
    assert_eq!(
        statements.len(),
        1,
        "simple SELECT expected one embedded SQL execution, observed:\n{}",
        statements.join("\n")
    );
    assert!(
        statements[0].contains("bookie_round_trip_budget_sqlite"),
        "SQL trace omitted measured query: {statements:#?}"
    );
}

struct QueryStatements(Arc<Mutex<Vec<String>>>);

impl<S: Subscriber> Layer<S> for QueryStatements {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let mut fields = QueryFields::default();
        event.record(&mut fields);
        let statement = format!(
            "{} {}",
            fields.summary.unwrap_or_default(),
            fields.statement.unwrap_or_default()
        );
        if statement.contains("bookie_round_trip_budget_sqlite") {
            self.0.lock().expect("SQL trace lock").push(statement);
        }
    }
}

#[derive(Default)]
struct QueryFields {
    summary: Option<String>,
    statement: Option<String>,
}

impl Visit for QueryFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value = format!("{value:?}");
        self.record(field.name(), value);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record(field.name(), value.to_string());
    }
}

impl QueryFields {
    fn record(&mut self, name: &str, value: String) {
        match name {
            "summary" => self.summary = Some(value),
            "db.statement" => self.statement = Some(value),
            _ => {}
        }
    }
}
