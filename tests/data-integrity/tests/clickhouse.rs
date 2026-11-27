#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_data_integrity::{clickhouse, judge_and_report, load_corpus, routes, runner};

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_date_destination_rejects_overflow_without_writing() {
    use bookie_core::{ColumnInfo, Connection, Value};
    use bookie_data_integrity::governed::{control, open_writable};
    let server = clickhouse::start("clickhouse-24.8", None).await;
    server
        .run_sql("CREATE TABLE date_guard (v Date, wide Date32) ENGINE = Memory")
        .await
        .unwrap();
    let column = |name: &str, data_type: &str| ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: false,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    };
    let columns = [column("v", "Date"), column("wide", "Date32")];
    let date = |text| Value::Date(chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap());
    let before = server.run_sql("SELECT count() FROM date_guard").await.unwrap();
    assert!(
        bookie_core::sql_dialect::build_insert_from_draft(
            "clickhouse",
            None,
            "date_guard",
            &columns,
            &[date("1900-01-01"), date("2299-12-31")]
        )
        .is_err()
    );
    assert_eq!(server.run_sql("SELECT count() FROM date_guard").await.unwrap(), before);
    let governed = open_writable("clickhouse", server.options.clone()).await;
    for narrow in ["1970-01-01", "2149-06-06"] {
        let (sql, params) = bookie_core::sql_dialect::build_insert_from_draft(
            "clickhouse",
            None,
            "date_guard",
            &columns,
            &[date(narrow), date("2299-12-31")],
        )
        .unwrap();
        governed
            .guard
            .execute_params_controlled(&sql, &params, &control())
            .await
            .unwrap();
    }
    assert_eq!(
        server
            .run_sql("SELECT toString(v), toString(wide) FROM date_guard ORDER BY v")
            .await
            .unwrap()
            .trim(),
        "1970-01-01\t2299-12-31\n2149-06-06\t2299-12-31"
    );
    governed.close().await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_float64_imports_preserve_server_values() {
    let mut corpus = load_corpus("clickhouse");
    corpus.cases.retain(|case| case.id == "float64");
    assert_eq!(corpus.cases.len(), 1);
    let server = clickhouse::start("clickhouse-24.8", None).await;
    let results = routes::run_routes(&server, &corpus, false).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_date32_parameters_preserve_server_values() {
    let mut corpus = load_corpus("clickhouse");
    corpus.cases.retain(|case| case.id == "date32");
    assert_eq!(corpus.cases.len(), 1);
    let server = clickhouse::start("clickhouse-24.8", None).await;
    let reads = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &reads).assert_clean();
    let routes = routes::run_routes(&server, &corpus, false).await;
    judge_and_report(&corpus, &routes).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_24_8_new_york_reads_match_the_server_rendering() {
    let corpus = load_corpus("clickhouse");
    let server = clickhouse::start("clickhouse-24.8-new-york", Some("America/New_York")).await;
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_24_8_reads_match_the_server_rendering() {
    let corpus = load_corpus("clickhouse");
    let server = clickhouse::start("clickhouse-24.8", None).await;
    let results = runner::run_reads(&server, &corpus).await;
    judge_and_report(&corpus, &results).assert_clean();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn clickhouse_24_8_copy_export_and_import_match_the_server() {
    let corpus = load_corpus("clickhouse");
    let server = clickhouse::start("clickhouse-24.8", None).await;
    let results = routes::run_routes(&server, &corpus, false).await;
    judge_and_report(&corpus, &results).assert_clean();
}
