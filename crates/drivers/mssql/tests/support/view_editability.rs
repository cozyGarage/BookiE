#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;

#[tokio::test]
#[ignore = "requires docker"]
async fn identifies_updatable_views_and_their_exposed_keys() {
    let (_container, options) = start_mssql().await;
    let connection = connect(options).await;
    connection
        .execute(
            "CREATE TABLE dbo.editable_base (id int NOT NULL, part int NOT NULL, name nvarchar(50), \
             CONSTRAINT PK_editable_base PRIMARY KEY (id, part)); \
             EXEC('CREATE VIEW dbo.editable_full AS SELECT id AS item_id, part, name, \
                   UPPER(name) AS upper_name FROM dbo.editable_base'); \
             EXEC('CREATE VIEW dbo.editable_partial AS SELECT id, name FROM dbo.editable_base'); \
             EXEC('CREATE VIEW dbo.editable_spoof AS SELECT id + 0 AS id, part, name FROM dbo.editable_base'); \
             EXEC('CREATE VIEW dbo.editable_aggregate AS SELECT id, part, name, COUNT(*) AS total \
                   FROM dbo.editable_base GROUP BY id, part, name');",
        )
        .await
        .unwrap();

    let bookie_core::ViewEditability::Editable {
        key_columns,
        read_only_columns,
        insertable,
    } = connection.fetch_view_editability(None, "editable_full").await.unwrap()
    else {
        panic!("a view exposing the complete primary key must be editable");
    };
    assert_eq!(key_columns, ["item_id", "part"]);
    assert_eq!(read_only_columns, ["upper_name"]);
    assert!(!insertable);
    assert_eq!(
        connection
            .fetch_view_editability(None, "editable_partial")
            .await
            .unwrap(),
        bookie_core::ViewEditability::KeyNotExposed
    );
    assert_eq!(
        connection.fetch_view_editability(None, "editable_spoof").await.unwrap(),
        bookie_core::ViewEditability::KeyNotExposed
    );
    assert_eq!(
        connection
            .fetch_view_editability(None, "editable_aggregate")
            .await
            .unwrap(),
        bookie_core::ViewEditability::NotUpdatable
    );
}
