#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tablepro_core::sql_ddl::{DraftColumn, build_alter_column};
use tablepro_core::{Connection, Value};

async fn mask_function(connection: &dyn Connection) -> Vec<Vec<Value>> {
    connection
        .query(
            "SELECT name, masking_function FROM sys.masked_columns \
             WHERE object_id = OBJECT_ID(N'[dbo].[people]') ORDER BY name",
        )
        .await
        .unwrap()
        .rows
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_dynamic_data_mask_survives_a_structure_tab_alter_of_the_column() {
    let (_container, options) = crate::start_mssql().await;
    let connection = crate::connect(options).await;
    connection
        .execute("CREATE TABLE [dbo].[people] (id INT PRIMARY KEY, email VARCHAR(50) MASKED WITH (FUNCTION = 'email()') NOT NULL)")
        .await
        .unwrap();
    let before = mask_function(connection.as_ref()).await;
    assert_eq!(
        before.len(),
        1,
        "the fixture must start with a masked column: {before:?}"
    );
    let columns = connection.fetch_columns(Some("dbo"), "people").await.unwrap();
    let mut draft = DraftColumn::from_info(columns.into_iter().find(|column| column.name == "email").unwrap());
    draft.nullable = true;

    for statement in build_alter_column("mssql", Some("dbo"), "people", &draft).unwrap() {
        connection.execute(&statement).await.unwrap();
    }

    assert_eq!(mask_function(connection.as_ref()).await, before);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_partial_mask_keeps_its_arguments_and_an_unmasked_column_still_alters() {
    let (_container, options) = crate::start_mssql().await;
    let connection = crate::connect(options).await;
    connection
        .execute(
            "CREATE TABLE [dbo].[people] (id INT PRIMARY KEY, \
             phone VARCHAR(20) MASKED WITH (FUNCTION = 'partial(2, \"xxx\", 1)') NOT NULL, note VARCHAR(20) NOT NULL)",
        )
        .await
        .unwrap();
    let before = mask_function(connection.as_ref()).await;
    let columns = connection.fetch_columns(Some("dbo"), "people").await.unwrap();

    for name in ["phone", "note"] {
        let mut draft = DraftColumn::from_info(columns.iter().find(|column| column.name == name).unwrap().clone());
        draft.nullable = true;
        for statement in build_alter_column("mssql", Some("dbo"), "people", &draft).unwrap() {
            connection.execute(&statement).await.unwrap();
        }
    }

    assert_eq!(mask_function(connection.as_ref()).await, before);
    let nullable = connection
        .query("SELECT name FROM sys.columns WHERE object_id = OBJECT_ID(N'[dbo].[people]') AND is_nullable = 1 ORDER BY name")
        .await
        .unwrap()
        .rows;
    assert_eq!(
        nullable,
        vec![vec![Value::Text("note".into())], vec![Value::Text("phone".into())]]
    );
}
