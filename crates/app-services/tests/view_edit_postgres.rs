#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bookie_app_services::view_edit::{ViewEditRestriction, resolve_view_columns};
use bookie_core::{
    ConnectOptions, Connection, DatabaseDriver, DriverError, Environment, OperationControl, Value, ViewEditability,
};
use bookie_policy::{
    AuditError, AuditEvent, AuditRecordPhase, AuditSink, AuditState, AuditTerminalStatus, AutoApproveSink,
    GuardContext, PolicyConfig, PolicyGuard, Principal,
};
use drivers_postgres::PgDriver;
use secrecy::SecretString;
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Default)]
struct CapturingAudit {
    events: Mutex<Vec<AuditEvent>>,
}

#[async_trait]
impl AuditSink for CapturingAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

const SEED: &[&str] = &[
    "CREATE TABLE items (id int PRIMARY KEY, name text NOT NULL, qty int)",
    "CREATE TABLE tags (item_id int, tag text)",
    "INSERT INTO items VALUES (1, 'bolt', 10), (2, 'nut', 20)",
    "CREATE VIEW v_items AS SELECT id, name FROM items",
    "CREATE VIEW v_alias AS SELECT id AS item_id, name || '!' AS shout, qty FROM items",
    "CREATE VIEW v_over_view AS SELECT id, name FROM v_items WHERE id > 0",
    "CREATE VIEW v_keyless AS SELECT name, qty FROM items",
    "CREATE VIEW v_aggregate AS SELECT name, count(*) AS n FROM items GROUP BY name",
    "CREATE VIEW v_join AS SELECT i.id, t.tag FROM items i JOIN tags t ON t.item_id = i.id",
    "CREATE VIEW v_trigger AS SELECT id, name FROM items",
    "CREATE FUNCTION v_trigger_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$",
    "CREATE TRIGGER v_trigger_instead INSTEAD OF UPDATE ON v_trigger FOR EACH ROW EXECUTE FUNCTION v_trigger_fn()",
    "CREATE VIEW v_rule AS SELECT id, name FROM items",
    "CREATE RULE v_rule_update AS ON UPDATE TO v_rule DO INSTEAD UPDATE items SET name = NEW.name WHERE id = OLD.id",
    "CREATE MATERIALIZED VIEW m_items AS SELECT id, name FROM items",
];

struct Fixture {
    _container: ContainerAsync<Postgres>,
    options: ConnectOptions,
    admin: Box<dyn Connection>,
}

async fn fixture(tag: &str) -> Fixture {
    let container = Postgres::default()
        .with_tag(tag.to_string())
        .start()
        .await
        .expect("start postgres container");
    let options = ConnectOptions {
        host: container.get_host().await.expect("host").to_string(),
        port: container.get_host_port_ipv4(5432).await.expect("port"),
        database: "postgres".into(),
        username: "postgres".into(),
        password: SecretString::from("postgres"),
        tls: bookie_core::TlsConfig::disabled(),
        ..Default::default()
    };
    let admin = PgDriver.connect(options.clone()).await.expect("connect");
    for statement in SEED {
        admin.execute(statement).await.expect(statement);
    }
    Fixture {
        _container: container,
        options,
        admin,
    }
}

async fn editability(fixture: &Fixture, name: &str) -> ViewEditability {
    fixture
        .admin
        .fetch_view_editability(Some("public"), name)
        .await
        .expect(name)
}

async fn assert_catalog_verdicts(fixture: &Fixture) {
    assert_eq!(editability(fixture, "items").await, ViewEditability::NotApplicable);
    assert_eq!(editability(fixture, "m_items").await, ViewEditability::NotApplicable);
    assert_eq!(
        editability(fixture, "v_items").await,
        ViewEditability::Editable {
            key_columns: vec!["id".into()],
            read_only_columns: Vec::new(),
            insertable: true,
        }
    );
    assert_eq!(
        editability(fixture, "v_alias").await,
        ViewEditability::Editable {
            key_columns: vec!["item_id".into()],
            read_only_columns: vec!["shout".into()],
            insertable: true,
        }
    );
    assert_eq!(
        editability(fixture, "v_over_view").await,
        ViewEditability::Editable {
            key_columns: vec!["id".into()],
            read_only_columns: Vec::new(),
            insertable: true,
        }
    );
    for keyless in ["v_keyless"] {
        assert_eq!(editability(fixture, keyless).await, ViewEditability::KeyNotExposed);
    }
    for refused in ["v_aggregate", "v_join", "v_trigger", "v_rule"] {
        assert_eq!(
            editability(fixture, refused).await,
            ViewEditability::NotUpdatable,
            "{refused}"
        );
    }
}

async fn guard(fixture: &Fixture, audit: Arc<CapturingAudit>, read_only: bool) -> PolicyGuard {
    let raw = PgDriver.connect(fixture.options.clone()).await.expect("connect");
    PolicyGuard::new(
        Arc::from(raw),
        GuardContext {
            connection_id: Uuid::nil(),
            connection_name: "view-edit-test".into(),
            driver_id: "postgres".into(),
            environment: Environment::Local,
            read_only,
            principal: Principal::human_gui(),
            policy: Arc::new(PolicyConfig::default()),
            approval: Arc::new(AutoApproveSink),
            audit,
            audit_state: Arc::new(AuditState::new()),
        },
    )
}

fn update_through(view: &str, key: &str) -> Vec<(String, Vec<Value>)> {
    vec![(
        format!("UPDATE \"public\".\"{view}\" SET \"name\" = $1 WHERE \"{key}\" = $2"),
        vec![Value::Text("washer".into()), Value::Int(1)],
    )]
}

async fn assert_edit_reaches_the_base_table(fixture: &Fixture) {
    let audit = Arc::new(CapturingAudit::default());
    let guard = guard(fixture, audit.clone(), false).await;
    let control = OperationControl::new(CancellationToken::new(), None);
    let counts = guard
        .execute_in_transaction_checked_controlled(&update_through("v_items", "id"), &[1], &control)
        .await
        .expect("save through the view");
    assert_eq!(counts, vec![1]);
    let stored = fixture
        .admin
        .query("SELECT name FROM items WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(stored.rows[0][0], Value::Text("washer".into()));
    let deleted = guard
        .execute_in_transaction_checked_controlled(
            &[(
                "DELETE FROM \"public\".\"v_items\" WHERE \"id\" = $1".into(),
                vec![Value::Int(2)],
            )],
            &[1],
            &control,
        )
        .await
        .expect("delete through the view");
    assert_eq!(deleted, vec![1]);
    let left = fixture.admin.query("SELECT count(*) FROM items").await.unwrap();
    assert_eq!(left.rows[0][0], Value::Int(1));
}

async fn assert_denied_save(fixture: &Fixture) {
    let audit = Arc::new(CapturingAudit::default());
    let guard = guard(fixture, audit.clone(), true).await;
    let control = OperationControl::new(CancellationToken::new(), None);
    let outcome = guard
        .execute_in_transaction_checked_controlled(&update_through("v_items", "id"), &[1], &control)
        .await;
    assert!(matches!(outcome, Err(DriverError::PolicyDenied(_))), "{outcome:?}");
    let denied = audit
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| {
            event.phase == AuditRecordPhase::Outcome && event.terminal_status == AuditTerminalStatus::Denied
        })
        .count();
    assert!(denied >= 1);
}

async fn assert_resolved_columns(fixture: &Fixture) {
    let control = OperationControl::new(CancellationToken::new(), None);
    let columns = fixture.admin.fetch_columns(Some("public"), "v_alias").await.unwrap();
    let plan = resolve_view_columns(fixture.admin.as_ref(), Some("public"), "v_alias", columns, &control).await;
    assert_eq!(plan.restriction, None);
    let keyed: Vec<&str> = plan
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| column.name.as_str())
        .collect();
    assert_eq!(keyed, ["item_id"]);
    let keyless = fixture.admin.fetch_columns(Some("public"), "v_keyless").await.unwrap();
    let plan = resolve_view_columns(fixture.admin.as_ref(), Some("public"), "v_keyless", keyless, &control).await;
    assert_eq!(plan.restriction, Some(ViewEditRestriction::KeyNotExposed));
    assert!(plan.columns.iter().all(|column| !column.primary_key));
    let aggregate = fixture
        .admin
        .fetch_columns(Some("public"), "v_aggregate")
        .await
        .unwrap();
    let plan = resolve_view_columns(
        fixture.admin.as_ref(),
        Some("public"),
        "v_aggregate",
        aggregate,
        &control,
    )
    .await;
    assert_eq!(plan.restriction, Some(ViewEditRestriction::NotUpdatable));
}

async fn run_for(tag: &str) {
    let fixture = fixture(tag).await;
    assert_catalog_verdicts(&fixture).await;
    assert_resolved_columns(&fixture).await;
    assert_denied_save(&fixture).await;
    assert_edit_reaches_the_base_table(&fixture).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test view_edit_postgres -- --include-ignored"]
async fn views_are_editable_only_when_the_catalog_proves_a_unique_row_key_on_postgres_16() {
    run_for("16-alpine").await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires Docker; run cargo test -p bookie-app-services --test view_edit_postgres -- --include-ignored"]
async fn views_are_editable_only_when_the_catalog_proves_a_unique_row_key_on_postgres_18() {
    run_for("18-alpine").await;
}
