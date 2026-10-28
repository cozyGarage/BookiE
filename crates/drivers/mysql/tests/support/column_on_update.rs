#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{connect, start_mysql};
use tablepro_core::Value;
use tablepro_core::sql_ddl::{DraftColumn, build_alter_column};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_structure_alter_keeps_on_update_current_timestamp() {
    let (_container, options) = start_mysql().await;
    let connection = connect(options).await;
    connection
        .execute(
            "CREATE TABLE audit (id INT PRIMARY KEY, updated_at TIMESTAMP(3) NOT NULL \
             DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3))",
        )
        .await
        .unwrap();
    let columns = connection.fetch_columns(None, "audit").await.unwrap();
    let mut draft = DraftColumn::from_info(columns.into_iter().find(|column| column.name == "updated_at").unwrap());
    draft.comment = Some("last write".into());

    for statement in build_alter_column("mysql", None, "audit", &draft).unwrap() {
        connection.execute(&statement).await.unwrap();
    }

    let extra = connection
        .query(
            "SELECT LOWER(extra) FROM information_schema.columns \
             WHERE table_schema = DATABASE() AND table_name = 'audit' AND column_name = 'updated_at'",
        )
        .await
        .unwrap()
        .rows;
    let Value::Text(extra) = &extra[0][0] else {
        panic!("extra is text: {extra:?}")
    };
    assert!(extra.contains("on update current_timestamp(3)"), "{extra}");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_structure_alter_keeps_the_srid_of_a_spatial_column() {
    let (_container, options) = start_mysql().await;
    let connection = connect(options).await;
    connection
        .execute("CREATE TABLE places (id INT PRIMARY KEY, spot POINT SRID 4326 NOT NULL)")
        .await
        .unwrap();
    let columns = connection.fetch_columns(None, "places").await.unwrap();
    let mut draft = DraftColumn::from_info(columns.into_iter().find(|column| column.name == "spot").unwrap());
    draft.comment = Some("where".into());

    for statement in build_alter_column("mysql", None, "places", &draft).unwrap() {
        connection.execute(&statement).await.unwrap();
    }

    let srid = connection
        .query(
            "SELECT srs_id FROM information_schema.st_geometry_columns \
             WHERE table_schema = DATABASE() AND table_name = 'places' AND column_name = 'spot'",
        )
        .await
        .unwrap()
        .rows;
    assert_eq!(srid, vec![vec![Value::Int(4326)]]);
}
