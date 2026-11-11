#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::sql_ddl::{DraftColumn, build_create_table_with_default_collation};
use bookie_core::{ConnectOptions, Value};

use crate::{connect, start_mariadb, start_mysql};

fn column(name: &str, data_type: &str, primary_key: bool, collation: Option<&str>) -> DraftColumn {
    DraftColumn {
        original: None,
        name: name.into(),
        data_type: data_type.into(),
        nullable: !primary_key,
        primary_key,
        auto_increment: false,
        default_value: None,
        comment: None,
        collation: collation.map(str::to_owned),
    }
}

async fn text_of(connection: &dyn bookie_core::Connection, sql: &str) -> String {
    let result = connection.query(sql).await.unwrap();
    match &result.rows[0][0] {
        Value::Text(text) => text.clone(),
        other => panic!("{sql} returned {other:?}"),
    }
}

async fn assert_the_default_collation_applies(options: ConnectOptions) {
    let connection = connect(options).await;
    let listed = connection.list_collations().await.unwrap();
    for wanted in ["utf8mb4_bin", "utf8mb4_unicode_ci"] {
        assert!(
            listed.iter().any(|collation| collation.name == wanted),
            "{wanted} must be listed"
        );
    }
    let columns = vec![
        column("id", "int", true, None),
        column("label", "varchar(20)", false, Some("utf8mb4_unicode_ci")),
        column("note", "varchar(20)", false, None),
    ];
    let statements =
        build_create_table_with_default_collation("mysql", None, "collated", &columns, &[], &[], Some("utf8mb4_bin"))
            .unwrap();
    for statement in &statements {
        connection.execute(statement).await.unwrap();
    }

    assert_eq!(
        text_of(
            connection.as_ref(),
            "SELECT TABLE_COLLATION FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'collated'"
        )
        .await,
        "utf8mb4_bin"
    );
    let column_collation = |name: &'static str| {
        format!(
            "SELECT COLLATION_NAME FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'collated' AND COLUMN_NAME = '{name}'"
        )
    };
    assert_eq!(
        text_of(connection.as_ref(), &column_collation("label")).await,
        "utf8mb4_unicode_ci",
        "a column's own collation wins over the table default"
    );
    assert_eq!(
        text_of(connection.as_ref(), &column_collation("note")).await,
        "utf8mb4_bin",
        "a column without its own collation inherits the table default"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_applies_a_new_tables_default_collation_and_its_column_collations() {
    let (_container, options) = start_mysql().await;
    assert_the_default_collation_applies(options).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mariadb_applies_a_new_tables_default_collation_and_its_column_collations() {
    let (_container, options) = start_mariadb().await;
    assert_the_default_collation_applies(options).await;
}
