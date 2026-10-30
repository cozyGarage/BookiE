#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{connect, start_pg};

#[tokio::test]
#[ignore = "requires docker"]
async fn enum_labels_come_back_in_declared_order_for_the_named_type_only() {
    let (_c, opts) = start_pg().await;
    let conn = connect(opts).await;
    conn.execute("CREATE SCHEMA other").await.unwrap();
    conn.execute("CREATE TYPE mood AS ENUM ('sad', 'ok', 'happy', 'it''s fine', '')")
        .await
        .unwrap();
    conn.execute("CREATE TYPE other.mood AS ENUM ('elsewhere')")
        .await
        .unwrap();

    let labels = conn.fetch_enum_labels("public", "mood").await.unwrap();
    assert_eq!(labels, ["sad", "ok", "happy", "it's fine", ""]);
    assert_eq!(conn.fetch_enum_labels("other", "mood").await.unwrap(), ["elsewhere"]);
    assert!(conn.fetch_enum_labels("public", "missing").await.unwrap().is_empty());

    conn.execute("ALTER TYPE mood ADD VALUE 'ecstatic' AFTER 'happy'")
        .await
        .unwrap();
    conn.execute("ALTER TYPE mood ADD VALUE 'miserable' BEFORE 'sad'")
        .await
        .unwrap();
    let ordered_labels = conn.fetch_enum_labels("public", "mood").await.unwrap();
    assert_eq!(
        ordered_labels,
        ["miserable", "sad", "ok", "happy", "ecstatic", "it's fine", ""]
    );

    conn.execute("CREATE DOMAIN mood_domain AS mood").await.unwrap();
    let domain_labels = conn.fetch_enum_labels("public", "mood_domain").await.unwrap();
    assert_eq!(domain_labels, ordered_labels);

    let query_res = conn.query("SELECT 'happy'::mood AS current_mood").await.unwrap();
    assert_eq!(
        query_res.columns[0].enum_type,
        Some(tablepro_core::QualifiedTypeName {
            schema: "public".into(),
            name: "mood".into(),
        })
    );
}
