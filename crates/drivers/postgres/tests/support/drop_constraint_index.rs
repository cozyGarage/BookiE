#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::sql_ddl::{StructureOp, TableState, materialize_ops};

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn dropping_the_index_of_a_unique_constraint_drops_the_constraint() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE uniq (id int PRIMARY KEY, email text, CONSTRAINT uniq_email UNIQUE (email))")
        .await
        .unwrap();
    conn.execute("CREATE INDEX uniq_plain ON uniq (id)").await.unwrap();
    let indexes = conn.fetch_indexes(None, "uniq").await.unwrap();
    let owned = indexes.iter().find(|index| index.name == "uniq_email").unwrap();
    assert_eq!(owned.constraint.as_deref(), Some("uniq_email"));
    let plain = indexes.iter().find(|index| index.name == "uniq_plain").unwrap();
    assert_eq!(plain.constraint, None);

    let ops: Vec<StructureOp> = [owned, plain]
        .into_iter()
        .map(|index| StructureOp::DropIndex {
            schema: None,
            table: "uniq".into(),
            index_name: index.name.clone(),
            constraint: index.constraint.clone(),
        })
        .collect();
    for statement in materialize_ops(&ops, "postgres").unwrap() {
        let outcome = conn.execute(&statement).await;
        assert!(outcome.is_ok(), "{statement}: {outcome:?}");
    }
    let left = conn.fetch_indexes(None, "uniq").await.unwrap();
    assert_eq!(left.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["uniq_pkey"]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn replacing_an_index_and_a_foreign_key_under_the_same_name_changes_the_table() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE swap_parent (id int PRIMARY KEY)")
        .await
        .unwrap();
    conn.execute("CREATE TABLE swap (id int PRIMARY KEY, a int, b int)")
        .await
        .unwrap();
    conn.execute("CREATE INDEX swap_idx ON swap (a)").await.unwrap();
    conn.execute("ALTER TABLE swap ADD CONSTRAINT swap_fk FOREIGN KEY (a) REFERENCES swap_parent (id)")
        .await
        .unwrap();
    let original_indexes = conn.fetch_indexes(None, "swap").await.unwrap();
    let original_keys = conn.fetch_foreign_keys(None, "swap").await.unwrap();

    let mut indexes = original_indexes.clone();
    let replaced = indexes.iter_mut().find(|index| index.name == "swap_idx").unwrap();
    replaced.columns = vec!["a".into(), "b".into()];
    replaced.unique = true;
    let mut keys = original_keys.clone();
    keys[0].on_delete = Some("CASCADE".into());

    let ops = bookie_core::sql_ddl::diff_to_ops(
        None,
        TableState {
            table: "swap",
            columns: &[],
            indexes: &original_indexes,
            fks: &original_keys,
        },
        TableState {
            table: "swap",
            columns: &[],
            indexes: &indexes,
            fks: &keys,
        },
    );
    assert_eq!(ops.len(), 4, "{ops:?}");
    for statement in materialize_ops(&ops, "postgres").unwrap() {
        let outcome = conn.execute(&statement).await;
        assert!(outcome.is_ok(), "{statement}: {outcome:?}");
    }

    let after = conn.fetch_indexes(None, "swap").await.unwrap();
    let index = after.iter().find(|index| index.name == "swap_idx").unwrap();
    assert_eq!(index.columns, ["a", "b"]);
    assert!(index.unique);
    let key = &conn.fetch_foreign_keys(None, "swap").await.unwrap()[0];
    assert_eq!(key.on_delete.as_deref(), Some("CASCADE"));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_expression_index_is_marked_and_rebuilt_from_its_definition() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    conn.execute("CREATE TABLE people (id int PRIMARY KEY, email text)")
        .await
        .unwrap();
    conn.execute("CREATE INDEX people_email_lower ON people (lower(email), id)")
        .await
        .unwrap();
    conn.execute("CREATE INDEX people_email ON people (email)")
        .await
        .unwrap();

    let indexes = conn.fetch_indexes(None, "people").await.unwrap();
    let functional = indexes.iter().find(|index| index.name == "people_email_lower").unwrap();
    assert!(functional.has_expression_key);
    assert_eq!(functional.columns, ["lower(email)", "id"]);
    let rebuilt = bookie_core::sql_ddl::build_create_index("postgres", None, "people", functional).unwrap();
    assert!(rebuilt.ends_with("USING btree (lower(email), id)"), "{rebuilt}");
    let plain = indexes.iter().find(|index| index.name == "people_email").unwrap();
    assert!(!plain.has_expression_key);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn expression_partial_gin_and_descending_indexes_survive_a_rebuild_in_another_schema() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    for statement in [
        "CREATE TABLE items (id int PRIMARY KEY, name text, tags text[])",
        "CREATE INDEX i_expression ON items (lower(name))",
        "CREATE INDEX i_gin ON items USING gin (tags)",
        "CREATE INDEX i_descending ON items (id DESC NULLS LAST) INCLUDE (name)",
        "CREATE UNIQUE INDEX i_partial ON items (name text_pattern_ops) WHERE id > 0",
        "CREATE SCHEMA copy",
        "CREATE TABLE copy.items (id int PRIMARY KEY, name text, tags text[])",
    ] {
        conn.execute(statement).await.unwrap();
    }

    let indexes = conn.fetch_indexes(Some("public"), "items").await.unwrap();
    for index in indexes.iter().filter(|index| !index.primary) {
        let rebuilt = bookie_core::sql_ddl::build_create_index("postgres", Some("copy"), "items", index).unwrap();
        conn.execute(&rebuilt).await.unwrap();
    }

    let definitions = conn
        .query(
            "SELECT pg_catalog.replace(pg_catalog.pg_get_indexdef(indexrelid), ' ON copy.', ' ON public.') \
             FROM pg_catalog.pg_index WHERE indrelid IN ('public.items'::regclass, 'copy.items'::regclass) \
             ORDER BY 1",
        )
        .await
        .unwrap();
    let texts: Vec<String> = definitions.rows.iter().map(|row| format!("{:?}", row[0])).collect();
    assert_eq!(texts.len(), 10, "{texts:?}");
    for pair in texts.chunks(2) {
        assert_eq!(pair[0], pair[1], "{texts:?}");
    }
}
