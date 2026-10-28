#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use drivers_postgres::PgDriver;
use secrecy::SecretString;
use tablepro_app_services::browse_query::{BrowseTarget, PageQuery, mark_partial_cells, parse_partial};
use tablepro_core::{ColumnInfo, ConnectOptions, DatabaseDriver, FilterSet, TlsConfig, Value};
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_huge_text_cell_arrives_as_a_small_partial_marker_and_a_normal_one_is_untouched() {
    let container = Postgres::default().with_tag("16-alpine").start().await.unwrap();
    let connection = PgDriver
        .connect(ConnectOptions {
            host: container.get_host().await.unwrap().to_string(),
            port: container.get_host_port_ipv4(5432).await.unwrap(),
            database: "postgres".into(),
            username: "postgres".into(),
            password: SecretString::from("postgres"),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE notes (id int PRIMARY KEY, body text, title varchar(40))")
        .await
        .unwrap();
    connection
        .execute("INSERT INTO notes VALUES (1, repeat('x', 2097152), 'big'), (2, 'short', 'small'), (3, NULL, NULL)")
        .await
        .unwrap();
    let columns = [
        column("id", "integer", true),
        column("body", "text", false),
        column("title", "character varying(40)", false),
    ];
    let filter = FilterSet::default();
    let target = BrowseTarget {
        driver_id: "postgres",
        schema: None,
        table: "notes",
        columns: &columns,
        filter: &filter,
        hidden_columns: None,
        long_text_preview: true,
    };
    let PageQuery::Sql(query) = target.page(0, 10, None, None).unwrap() else {
        panic!("a preview page must run as SQL, not through the driver's whole-row fetch")
    };

    let mut result = connection.query_params(&query.sql, &query.params).await.unwrap();
    let transferred: usize = result
        .rows
        .iter()
        .flatten()
        .map(|cell| if let Value::Text(text) = cell { text.len() } else { 0 })
        .sum();
    mark_partial_cells(&mut result);

    assert!(
        transferred < 16 * 1024,
        "only a small preview may cross the wire: {transferred}"
    );
    let Value::Undecodable(partial) = &result.rows[0][1] else {
        panic!("the huge cell is partial: {:?}", result.rows[0][1])
    };
    assert_eq!(parse_partial(partial), Some((2_097_152, "x".repeat(1024).as_str())));
    assert_eq!(result.rows[1][1], Value::Text("short".into()));
    assert_eq!(result.rows[2][1], Value::Null);
    assert_eq!(result.rows[0][2], Value::Text("big".into()));
}
