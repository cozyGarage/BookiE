#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;

#[tokio::test]
#[ignore = "requires docker"]
async fn an_error_raised_after_the_first_result_set_is_reported() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;

    let err = conn
        .query("SELECT 1 AS a; SELECT 2 AS b; SELECT 1/0 AS c")
        .await
        .unwrap_err();
    assert!(format!("{err}").to_lowercase().contains("divide by zero"), "got: {err}");

    let first = conn.query("SELECT 1 AS a; SELECT 'x' AS b, 'y' AS c").await.unwrap();
    let names: Vec<&str> = first.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["a"]);
    assert_eq!(first.rows, vec![vec![Value::Int(1)]]);

    let after_late_error = conn.query("SELECT 3 AS usable").await.unwrap();
    assert_eq!(
        after_late_error.rows,
        vec![vec![Value::Int(3)]],
        "draining later result sets leaves the connection usable"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn only_the_first_result_set_is_returned_even_when_it_has_no_rows() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;

    let result = conn
        .query("SELECT CAST('first' AS nvarchar(10)) AS first_set WHERE 1 = 0; SELECT 2 AS later_set")
        .await
        .unwrap();
    assert!(result.rows.is_empty());
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0].name, "first_set");
    assert_eq!(result.columns[0].data_type, "nvarchar");
    assert_eq!(
        conn.query("SELECT 3 AS usable").await.unwrap().rows,
        vec![vec![Value::Int(3)]]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_result_cut_at_the_memory_budget_reports_truncation_and_drains_the_batch() {
    let (_container, opts) = start_mssql().await;
    let conn = connect(opts).await;
    let rows = 70;
    let bytes_per_row = tablepro_core::MAX_QUERY_RESULT_BYTES / 64 + 1;
    let result = conn
        .query(&format!(
            "SELECT TOP ({rows}) REPLICATE(CAST('x' AS varchar(max)), {bytes_per_row}) AS payload FROM sys.all_objects a CROSS JOIN sys.all_objects b"
        ))
        .await
        .unwrap();

    assert!(result.truncated);
    assert!(!result.rows.is_empty());
    assert!(result.rows.len() < rows);
    assert_eq!(result.rows[0][0], Value::Text("x".repeat(bytes_per_row)));
    assert_eq!(
        conn.query("SELECT 4 AS usable").await.unwrap().rows,
        vec![vec![Value::Int(4)]]
    );
}
