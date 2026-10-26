#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use tablepro_core::{ColumnInfo, DriverError, OperationControl, Session};
use tokio_util::sync::CancellationToken;

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_bare_table_name_resolves_through_the_whole_search_path() {
    let (_c, opts) = start_pg().await;
    let setup = connect(opts.clone()).await;
    for statement in [
        "CREATE SCHEMA first_ns",
        "CREATE SCHEMA second_ns",
        "CREATE TABLE second_ns.only_second (x integer PRIMARY KEY)",
        "CREATE TABLE first_ns.shared (in_first integer)",
        "CREATE TABLE second_ns.shared (in_second integer)",
        "CREATE TABLE second_ns.child_second (x integer REFERENCES second_ns.only_second (x))",
        "ALTER DATABASE test SET search_path = first_ns, second_ns",
    ] {
        setup.execute(statement).await.unwrap();
    }
    drop(setup);
    let conn = connect(opts).await;

    let names = |columns: Vec<ColumnInfo>| columns.into_iter().map(|c| c.name).collect::<Vec<_>>();
    assert_eq!(names(conn.fetch_columns(None, "only_second").await.unwrap()), ["x"]);
    assert_eq!(names(conn.fetch_columns(None, "shared").await.unwrap()), ["in_first"]);
    assert_eq!(conn.fetch_indexes(None, "only_second").await.unwrap().len(), 1);
    let foreign_keys = conn.fetch_foreign_keys(None, "child_second").await.unwrap();
    assert_eq!(foreign_keys.len(), 1, "foreign keys resolve the bare name too");
    assert!(conn.fetch_columns(None, "").await.unwrap().is_empty());
    assert_eq!(
        names(conn.fetch_columns(Some("second_ns"), "shared").await.unwrap()),
        ["in_second"]
    );
    assert!(conn.fetch_columns(None, "no_such_table").await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "requires docker"]
async fn bare_metadata_lookup_uses_the_implicitly_visible_catalog_relation() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    connection
        .execute("CREATE TABLE public.catalog_reference (id integer PRIMARY KEY)")
        .await
        .unwrap();
    connection
        .execute("CREATE TABLE public.pg_class (shadow_column integer REFERENCES public.catalog_reference (id))")
        .await
        .unwrap();

    let bare_columns = connection.fetch_columns(None, "pg_class").await.unwrap();
    let bare_indexes = connection.fetch_indexes(None, "pg_class").await.unwrap();
    let bare_foreign_keys = connection.fetch_foreign_keys(None, "pg_class").await.unwrap();
    let qualified_columns = connection.fetch_columns(Some("public"), "pg_class").await.unwrap();
    let qualified_indexes = connection.fetch_indexes(Some("public"), "pg_class").await.unwrap();
    let qualified_foreign_keys = connection.fetch_foreign_keys(Some("public"), "pg_class").await.unwrap();

    assert!(bare_columns.iter().any(|column| column.name == "relname"));
    assert!(!bare_columns.iter().any(|column| column.name == "shadow_column"));
    assert!(!bare_indexes.is_empty());
    assert!(bare_foreign_keys.is_empty());
    assert_eq!(qualified_columns.len(), 1);
    assert_eq!(qualified_columns[0].name, "shadow_column");
    assert!(qualified_indexes.is_empty());
    assert_eq!(qualified_foreign_keys.len(), 1);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn completion_metadata_uses_each_session_search_path_and_temp_schema() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    for statement in [
        "CREATE SCHEMA first_ns",
        "CREATE SCHEMA second_ns",
        "CREATE TABLE first_ns.shared (first_only integer)",
        "CREATE TABLE second_ns.shared (second_only integer)",
        "CREATE TABLE public.pg_class (public_only integer)",
    ] {
        connection.execute(statement).await.unwrap();
    }
    let mut first = connection.open_session().await.unwrap();
    let mut second = connection.open_session().await.unwrap();
    run_session_statement(&mut first, "SET search_path TO first_ns, public").await;
    run_session_statement(&mut second, "SET search_path TO second_ns, public").await;
    run_session_statement(&mut first, "CREATE TEMP TABLE shared (temporary_only integer)").await;

    let first_columns = completion_columns(&mut first, None, "shared").await.unwrap();
    let second_columns = completion_columns(&mut second, None, "shared").await.unwrap();
    let qualified_columns = completion_columns(&mut first, Some("second_ns"), "shared")
        .await
        .unwrap();
    let catalog_columns = completion_columns(&mut first, None, "pg_class").await.unwrap();
    let missing_columns = completion_columns(&mut first, None, "no_such_table").await.unwrap();
    let repeated_columns = completion_columns(&mut first, None, "shared").await.unwrap();

    assert_eq!(first_columns, ["temporary_only"]);
    assert_eq!(second_columns, ["second_only"]);
    assert_eq!(qualified_columns, ["second_only"]);
    assert!(catalog_columns.iter().any(|column| column == "relname"));
    assert!(!catalog_columns.iter().any(|column| column == "public_only"));
    assert!(missing_columns.is_empty());
    assert_eq!(repeated_columns, first_columns);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn completion_metadata_obeys_cancellation_and_deadlines() {
    let (_container, options) = start_pg().await;
    let connection = connect(options).await;
    let mut session = connection.open_session().await.unwrap();
    let cancelled_token = CancellationToken::new();
    cancelled_token.cancel();
    let cancelled = session
        .fetch_completion_columns_controlled(None, "shared", &OperationControl::new(cancelled_token, None))
        .await;
    let expired = session
        .fetch_completion_columns_controlled(
            None,
            "shared",
            &OperationControl::new(
                CancellationToken::new(),
                Some(tokio::time::Instant::now() - Duration::from_secs(1)),
            ),
        )
        .await;
    assert!(matches!(cancelled, Err(DriverError::Cancelled)));
    assert!(matches!(expired, Err(DriverError::TimedOut)));
    assert!(completion_columns(&mut session, None, "pg_class").await.is_ok());
}

async fn completion_columns(
    session: &mut Box<dyn Session>,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<String>, DriverError> {
    session
        .fetch_completion_columns_controlled(schema, table, &OperationControl::with_timeout(Duration::from_secs(5)))
        .await
}

async fn run_session_statement(session: &mut Box<dyn Session>, statement: &str) {
    session
        .query_params_controlled(statement, &[], &OperationControl::with_timeout(Duration::from_secs(5)))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_column_collation_is_read_and_survives_a_type_change() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE collated (label varchar(40) COLLATE \"C\", plain text)")
        .await
        .unwrap();

    let columns = conn.fetch_columns(None, "collated").await.unwrap();
    assert_eq!(columns[0].collation.as_deref(), Some("C"));
    assert_eq!(columns[1].collation, None);

    let mut draft = tablepro_core::sql_ddl::DraftColumn::from_info(columns[0].clone());
    draft.data_type = "character varying(80)".into();
    for statement in tablepro_core::sql_ddl::build_alter_column("postgres", None, "collated", &draft).unwrap() {
        conn.execute(&statement).await.unwrap();
    }

    let altered = conn.fetch_columns(None, "collated").await.unwrap();
    assert_eq!(altered[0].data_type, "character varying(80)");
    assert_eq!(altered[0].collation.as_deref(), Some("C"));
}
