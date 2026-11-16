#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use bookie_core::{ConnectOptions, Connection, DatabaseDriver, Environment, TlsConfig, Value};
use bookie_policy::{AuditState, DenyApprovalSink, GuardContext, NullAuditSink, PolicyConfig, PolicyGuard, Principal};
use drivers_oracle::OracleDriver;
use rust_decimal::Decimal;
use secrecy::SecretString;
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt, core::IntoContainerPort, core::WaitFor, runners::AsyncRunner,
};
use uuid::Uuid;

const ADMIN_PASSWORD: &str = "bookie_policy_admin";
const APP_PASSWORD: &str = "bookie_policy_app";

#[tokio::test]
#[ignore = "requires docker"]
async fn a_denied_oracle_stored_function_does_not_commit_its_autonomous_write() {
    let (_container, port) = start_oracle().await;
    seed(port).await;

    let connection = OracleDriver
        .connect(options(port, "BOOKIE_IT", APP_PASSWORD))
        .await
        .expect("connect application user");
    let context = GuardContext {
        connection_id: Uuid::new_v4(),
        connection_name: "oracle-policy-container-test".into(),
        driver_id: OracleDriver.id().into(),
        environment: Environment::Local,
        read_only: false,
        principal: Principal::human_gui(),
        policy: Arc::new(PolicyConfig::default()),
        approval: Arc::new(DenyApprovalSink),
        audit: Arc::new(NullAuditSink),
        audit_state: Arc::new(AuditState::new()),
    };
    let guard = PolicyGuard::new(Arc::from(connection), context);

    let error = guard
        .query("SELECT bookie_it.delayed_write() FROM dual")
        .await
        .expect_err("the stored function must be denied before execution");

    assert!(matches!(error, bookie_core::DriverError::PolicyDenied(_)), "{error:?}");
    let connection = OracleDriver
        .connect(options(port, "BOOKIE_IT", APP_PASSWORD))
        .await
        .expect("reconnect to inspect the marker table");
    let result = connection
        .query("SELECT COUNT(*) FROM bookie_it.marker")
        .await
        .expect("read marker count after denial");
    assert_eq!(result.rows.iter().next().unwrap()[0], Value::Decimal(Decimal::ZERO));
}

async fn start_oracle() -> (ContainerAsync<GenericImage>, u16) {
    let image = GenericImage::new("gvenzl/oracle-free", "slim-faststart")
        .with_exposed_port(1521.tcp())
        .with_wait_for(WaitFor::message_on_stdout("DATABASE IS READY TO USE!"))
        .with_env_var("ORACLE_PASSWORD", ADMIN_PASSWORD);
    let image = match std::env::var("BOOKIE_TEST_RUN_ID") {
        Ok(run_id) => image.with_label("com.bookie.test-run", run_id),
        Err(_) => image,
    };
    let container = image.start().await.expect("start Oracle Free");
    let port = container.get_host_port_ipv4(1521).await.expect("get Oracle port");
    (container, port)
}

fn options(port: u16, username: &str, password: &str) -> ConnectOptions {
    ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        database: "FREEPDB1".into(),
        username: username.into(),
        password: SecretString::new(password.to_string().into()),
        tls: TlsConfig::disabled(),
        ..ConnectOptions::default()
    }
}

async fn seed(port: u16) {
    tokio::task::spawn_blocking(move || {
        let config = oracledb::Config::default()
            .set_credentials("system", ADMIN_PASSWORD)
            .set_connect_string(&format!("127.0.0.1:{port}/FREEPDB1"))
            .unwrap();
        let admin = oracledb::connect(config).unwrap();
        for statement in [
            "CREATE USER BOOKIE_IT IDENTIFIED BY bookie_policy_app QUOTA UNLIMITED ON USERS",
            "GRANT CREATE SESSION, CREATE TABLE, CREATE PROCEDURE TO BOOKIE_IT",
            "CREATE TABLE BOOKIE_IT.MARKER (ID NUMBER PRIMARY KEY)",
            "CREATE OR REPLACE FUNCTION BOOKIE_IT.DELAYED_WRITE RETURN NUMBER IS \
             PRAGMA AUTONOMOUS_TRANSACTION; BEGIN INSERT INTO BOOKIE_IT.MARKER (ID) VALUES (1); \
             COMMIT; RETURN 1; END;",
        ] {
            admin.execute(statement, &[]).unwrap();
        }
    })
    .await
    .unwrap();
}
