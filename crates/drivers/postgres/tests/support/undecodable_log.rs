use std::sync::{Arc, Mutex};

use bookie_core::Value;
use tracing_subscriber::fmt::MakeWriter;

use crate::{connect, no_timeout, start_pg};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires docker"]
async fn cursor_logs_one_count_for_all_pages_and_preserves_values() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    let mut session = connection.open_session().await.unwrap();
    let control = no_timeout();
    session
        .query_params_controlled("BEGIN READ ONLY", &[], &control)
        .await
        .unwrap();
    let logs = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let cursor = session
        .open_query_cursor_controlled(
            "SELECT numrange(1.5, 3.5) FROM generate_series(1, 10001)",
            &[],
            &control,
        )
        .await
        .unwrap();
    let mut rows = 0;
    loop {
        let page = session
            .fetch_query_cursor_page_controlled(cursor, &control)
            .await
            .unwrap();
        assert!(
            page.result
                .rows
                .iter()
                .all(|row| matches!(&row[0], Value::Undecodable(name) if name.eq_ignore_ascii_case("numrange"))),
            "{:?}",
            page.result.rows.first()
        );
        rows += page.result.rows.len();
        if !page.has_next_page {
            break;
        }
    }
    session.close_query_cursor_controlled(cursor, &control).await.unwrap();
    assert_eq!(rows, 10001);
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    let warnings: Vec<_> = text
        .lines()
        .filter(|line| line.contains("postgres column values could not be decoded"))
        .collect();
    assert_eq!(warnings.len(), 1, "{text}");
    assert!(warnings[0].contains("column_index=1"), "{text}");
    assert!(warnings[0].contains("count=10001"), "{text}");
    assert!(warnings[0].to_ascii_lowercase().contains("numrange"), "{text}");
}
