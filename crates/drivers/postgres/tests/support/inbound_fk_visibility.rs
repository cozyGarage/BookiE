use crate::{connect, start_pg};
use secrecy::SecretString;

#[tokio::test]
#[ignore = "requires docker"]
async fn inbound_keys_hide_unreachable_referencing_tables() {
    let (_container, mut options) = start_pg().await;
    let admin = connect(options.clone()).await;
    let role = format!("fk_view_{}", uuid::Uuid::new_v4().simple());
    for sql in [
        format!("CREATE ROLE {role} LOGIN PASSWORD 'fixture'"),
        "CREATE TABLE public.fk_target (id int PRIMARY KEY)".into(),
        "CREATE TABLE public.fk_reachable (id int REFERENCES public.fk_target)".into(),
        "CREATE TABLE public.fk_hidden (id int REFERENCES public.fk_target)".into(),
        "CREATE TABLE public.fk_partial (id int REFERENCES public.fk_target, secret int)".into(),
        "CREATE TABLE public.fk_denied_target (id int PRIMARY KEY)".into(),
        "CREATE TABLE public.fk_other (id int REFERENCES public.fk_denied_target)".into(),
        "CREATE SCHEMA fk_hidden_schema".into(),
        "CREATE TABLE fk_hidden_schema.fk_no_schema (id int REFERENCES public.fk_target)".into(),
        format!("GRANT SELECT ON public.fk_target, public.fk_reachable, public.fk_other TO {role}"),
        format!("GRANT SELECT (id) ON public.fk_partial, fk_hidden_schema.fk_no_schema TO {role}"),
    ] {
        admin.execute(&sql).await.unwrap();
    }
    options.username = role.clone();
    options.password = SecretString::new("fixture".into());
    let restricted = connect(options).await;
    let mut results = Vec::new();
    for schema in [None, Some("public")] {
        let keys = restricted
            .fetch_inbound_foreign_keys(schema, "fk_target")
            .await
            .unwrap();
        results.push(keys.into_iter().map(|key| key.source_table).collect::<Vec<_>>());
    }
    let inaccessible_target = restricted
        .fetch_inbound_foreign_keys(Some("public"), "fk_denied_target")
        .await
        .unwrap();
    let all = admin
        .fetch_inbound_foreign_keys(Some("public"), "fk_target")
        .await
        .unwrap();
    drop(restricted);
    for sql in [
        "DROP SCHEMA fk_hidden_schema CASCADE".into(),
        "DROP TABLE public.fk_reachable, public.fk_hidden, public.fk_partial, public.fk_target, public.fk_other, public.fk_denied_target".into(),
        format!("DROP ROLE {role}"),
    ] {
        admin.execute(&sql).await.unwrap();
    }
    assert!(inaccessible_target.is_empty());
    assert_eq!(all.len(), 4, "the admin sees all source tables");
    for sources in results {
        assert_eq!(sources, ["fk_partial", "fk_reachable"]);
    }
}
