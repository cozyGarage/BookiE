#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::{ConnectOptions, Connection, Value};

use crate::{connect, start_mariadb, start_mysql};

const MODES: [&str; 4] = [
    "",
    "NO_BACKSLASH_ESCAPES",
    "ANSI_QUOTES",
    "ANSI_QUOTES,NO_BACKSLASH_ESCAPES",
];

async fn connect_in_mode(options: &ConnectOptions, mode: &str) -> Box<dyn Connection> {
    let admin = connect(options.clone()).await;
    admin.execute(&format!("SET GLOBAL sql_mode = '{mode}'")).await.unwrap();
    admin.close().await.unwrap();
    let connection = connect(options.clone()).await;
    let active = connection.query("SELECT @@SESSION.sql_mode").await.unwrap().rows[0][0].clone();
    let Value::Text(active) = active else {
        panic!("unexpected sql_mode: {active:?}")
    };
    assert_eq!(
        active.split(',').any(|value| value.trim() == "NO_BACKSLASH_ESCAPES"),
        mode.split(',').any(|value| value.trim() == "NO_BACKSLASH_ESCAPES"),
        "requested sql_mode {mode:?}, got {active:?}"
    );
    assert_eq!(
        active.split(',').any(|value| value.trim() == "ANSI_QUOTES"),
        mode.split(',').any(|value| value.trim() == "ANSI_QUOTES"),
        "ANSI_QUOTES mode mismatch: {active}"
    );
    connection
}

async fn assert_enum_literal_export_modes(options: ConnectOptions) {
    let setup = connect(options.clone()).await;
    setup
        .execute(
            r#"CREATE TABLE enum_mode_source (
                id INT PRIMARY KEY,
                mood ENUM('happy', 'it''s ok', 'back\\slash', 'NULL', '')
            )"#,
        )
        .await
        .unwrap();
    setup
        .execute("CREATE TABLE enum_mode_copy LIKE enum_mode_source")
        .await
        .unwrap();
    for (id, label) in [
        (1, Some("happy")),
        (2, Some("it's ok")),
        (3, Some("back\\slash")),
        (4, Some("NULL")),
        (5, Some("")),
        (6, None),
    ] {
        setup
            .execute_params(
                "INSERT INTO enum_mode_source VALUES (?, ?)",
                &[
                    Value::Int(id),
                    label.map_or(Value::Null, |value| Value::Text(value.into())),
                ],
            )
            .await
            .unwrap();
    }
    setup.close().await.unwrap();

    for mode in MODES {
        let connection = connect_in_mode(&options, mode).await;
        connection.execute("DELETE FROM enum_mode_copy").await.unwrap();
        let rows = connection
            .query("SELECT id, mood FROM enum_mode_source ORDER BY id")
            .await
            .unwrap()
            .rows;
        let native = connection
            .query("SELECT id, CAST(mood + 0 AS CHAR), HEX(mood) FROM enum_mode_source ORDER BY id")
            .await
            .unwrap();
        assert_eq!(
            native.rows,
            vec![
                vec![Value::Int(1), Value::Text("1".into()), Value::Text("6861707079".into())],
                vec![
                    Value::Int(2),
                    Value::Text("2".into()),
                    Value::Text("69742773206F6B".into())
                ],
                vec![
                    Value::Int(3),
                    Value::Text("3".into()),
                    Value::Text("6261636B5C736C617368".into())
                ],
                vec![Value::Int(4), Value::Text("4".into()), Value::Text("4E554C4C".into())],
                vec![Value::Int(5), Value::Text("5".into()), Value::Text(String::new())],
                vec![Value::Int(6), Value::Null, Value::Null],
            ],
            "native ENUM ordinals and labels in sql_mode {mode:?}"
        );
        let columns = connection.fetch_columns(None, "enum_mode_copy").await.unwrap();
        for row in &rows {
            let statement =
                tablepro_core::sql_literal::build_insert_literal("mysql", None, "enum_mode_copy", &columns, row)
                    .unwrap();
            connection
                .execute(&statement)
                .await
                .unwrap_or_else(|error| panic!("sql_mode {mode:?}; {statement}: {error}"));
        }
        let matching = connection
            .query(
                "SELECT COUNT(*) FROM enum_mode_source s JOIN enum_mode_copy c ON s.id = c.id \
                 AND s.mood + 0 <=> c.mood + 0 AND HEX(s.mood) <=> HEX(c.mood)",
            )
            .await
            .unwrap();
        assert_eq!(matching.rows, vec![vec![Value::Int(6)]], "sql_mode {mode:?}");
        connection.close().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_enum_sql_literals_survive_mysql_sql_modes() {
    let (_container, options) = start_mysql().await;
    assert_enum_literal_export_modes(options).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_enum_sql_literals_survive_mariadb_sql_modes() {
    let (_container, options) = start_mariadb().await;
    assert_enum_literal_export_modes(options).await;
}
