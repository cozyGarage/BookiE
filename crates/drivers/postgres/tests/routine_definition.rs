#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{
    ConnectOptions, Connection, DatabaseDriver, DriverError, ROUTINE_DEFINITION_LIMIT_BYTES, RoutineLookup, RoutineRef,
    TlsConfig,
};
use drivers_postgres::PgDriver;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres as PostgresImage;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

fn options(host: &str, port: u16, user: &str, password: &str) -> ConnectOptions {
    ConnectOptions {
        host: host.into(),
        port,
        database: "postgres".into(),
        username: user.into(),
        password: secrecy::SecretString::new(password.to_string().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    }
}

fn routine(schema: &str, name: &str, arguments: Option<&str>) -> RoutineRef {
    RoutineRef {
        schema: schema.into(),
        name: name.into(),
        identity_arguments: arguments.map(str::to_string),
    }
}

async fn seed(admin: &dyn Connection) {
    let long_body = "x".repeat(ROUTINE_DEFINITION_LIMIT_BYTES + 2_000);
    let statements = vec![
        "CREATE ROLE rd_restricted LOGIN PASSWORD 'restricted'".to_string(),
        "CREATE SCHEMA rd_open".into(),
        "CREATE SCHEMA rd_closed".into(),
        "CREATE SCHEMA \"Odd Schema\"".into(),
        "GRANT USAGE ON SCHEMA rd_open, \"Odd Schema\" TO rd_restricted".into(),
        "CREATE FUNCTION rd_open.add(a int, b int) RETURNS int LANGUAGE sql AS 'SELECT a + b'".into(),
        "CREATE FUNCTION rd_open.over(a int) RETURNS int LANGUAGE sql AS 'SELECT a'".into(),
        "CREATE FUNCTION rd_open.over(a text) RETURNS text LANGUAGE sql AS 'SELECT a'".into(),
        "CREATE PROCEDURE rd_open.touch(a int) LANGUAGE sql AS 'SELECT a'".into(),
        "CREATE FUNCTION rd_open.shut() RETURNS int LANGUAGE sql AS 'SELECT 1'".into(),
        "REVOKE EXECUTE ON FUNCTION rd_open.shut() FROM PUBLIC".into(),
        "CREATE FUNCTION rd_closed.secret() RETURNS int LANGUAGE sql AS 'SELECT 42'".into(),
        format!("CREATE FUNCTION rd_open.long_body() RETURNS text LANGUAGE sql AS $$ SELECT '{long_body}' $$"),
        "CREATE FUNCTION \"Odd Schema\".\"Odd Name\"() RETURNS int LANGUAGE sql AS 'SELECT 7'".into(),
        "CREATE AGGREGATE rd_open.agg_sum(int) (SFUNC = int4pl, STYPE = int, INITCOND = '0')".into(),
    ];
    for statement in statements {
        admin.execute(&statement).await.expect(&statement);
    }
}

fn found(lookup: RoutineLookup) -> bookie_core::RoutineDefinition {
    match lookup {
        RoutineLookup::Found(definition) => definition,
        other => panic!("expected a definition, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn routine_definitions_resolve_by_identity_and_honour_role_visibility() {
    let container = PostgresImage::default().with_tag("16-alpine").start().await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin = PgDriver
        .connect(options(&host, port, "postgres", "postgres"))
        .await
        .unwrap();
    seed(admin.as_ref()).await;
    let restricted = PgDriver
        .connect(options(&host, port, "rd_restricted", "restricted"))
        .await
        .unwrap();

    let add = found(
        admin
            .routine_definition(&routine("rd_open", "add", None))
            .await
            .unwrap(),
    );
    assert_eq!(add.kind, "function");
    assert_eq!(add.identity_arguments, "a integer, b integer");
    assert!(
        add.definition.contains("CREATE OR REPLACE FUNCTION rd_open.add"),
        "{}",
        add.definition
    );
    assert!(!add.truncated);

    let overloaded = admin
        .routine_definition(&routine("rd_open", "over", None))
        .await
        .unwrap();
    assert_eq!(
        overloaded,
        RoutineLookup::Ambiguous {
            signatures: vec!["a integer".into(), "a text".into()]
        }
    );
    let by_text = found(
        admin
            .routine_definition(&routine("rd_open", "over", Some("a text")))
            .await
            .unwrap(),
    );
    assert!(by_text.definition.contains("RETURNS text"), "{}", by_text.definition);

    let procedure = found(
        admin
            .routine_definition(&routine("rd_open", "touch", None))
            .await
            .unwrap(),
    );
    assert_eq!(procedure.kind, "procedure");

    let long = found(
        admin
            .routine_definition(&routine("rd_open", "long_body", None))
            .await
            .unwrap(),
    );
    assert!(long.truncated);
    assert!(long.definition.len() <= ROUTINE_DEFINITION_LIMIT_BYTES);
    assert!(long.total_bytes.unwrap() > ROUTINE_DEFINITION_LIMIT_BYTES as u64);

    let odd = found(
        admin
            .routine_definition(&routine("Odd Schema", "Odd Name", None))
            .await
            .unwrap(),
    );
    assert!(odd.definition.contains("SELECT 7"));

    let aggregate = admin.routine_definition(&routine("rd_open", "agg_sum", None)).await;
    assert!(matches!(aggregate, Err(DriverError::Unsupported(_))), "{aggregate:?}");

    let hostile = routine("rd_open", "add' OR '1'='1", None);
    assert_eq!(
        admin.routine_definition(&hostile).await.unwrap(),
        RoutineLookup::NotFound
    );

    assert!(matches!(
        restricted
            .routine_definition(&routine("rd_open", "add", None))
            .await
            .unwrap(),
        RoutineLookup::Found(_)
    ));
    for hidden in [routine("rd_closed", "secret", None), routine("rd_open", "shut", None)] {
        assert_eq!(
            restricted.routine_definition(&hidden).await.unwrap(),
            RoutineLookup::NotFound,
            "{hidden:?}"
        );
        assert!(matches!(
            admin.routine_definition(&hidden).await.unwrap(),
            RoutineLookup::Found(_)
        ));
    }
    let missing = restricted
        .routine_definition(&routine("rd_closed", "nothing", None))
        .await
        .unwrap();
    assert_eq!(
        missing,
        RoutineLookup::NotFound,
        "an absent and a hidden routine look the same"
    );
}
