use drivers_mongodb::MongodbDriver;
use mongodb::bson::{Bson, Document, doc, oid::ObjectId};
use tablepro_core::{DatabaseDriver, Value};

#[tokio::test]
#[ignore = "requires docker"]
async fn keyset_page_query_reads_rows_after_the_native_id_cursor() {
    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .expect("connect native fixture client");
    client
        .database("appdb")
        .collection::<Document>("keyset_page")
        .insert_many([
            doc! { "_id": 104, "value": "fourth" },
            doc! { "_id": 101, "value": "first" },
            doc! { "_id": 103, "value": "third" },
            doc! { "_id": 102, "value": "second" },
        ])
        .await
        .expect("seed keyset page");

    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();
    let page = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"keyset_page\" WHERE \"_id\" > ? LIMIT 2 OFFSET 0",
            &[Value::Int(100)],
        )
        .await
        .unwrap();

    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.rows[0][0], Value::Int(101));
    assert_eq!(page.rows[1][0], Value::Int(102));
}

#[tokio::test]
#[ignore = "requires docker"]
async fn keyset_page_query_binds_object_id_cursor_values() {
    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .unwrap();
    let first = ObjectId::parse_str("000000000000000000000001").unwrap();
    let second = ObjectId::parse_str("000000000000000000000002").unwrap();
    let third = ObjectId::parse_str("000000000000000000000003").unwrap();
    client
        .database("appdb")
        .collection::<Document>("keyset_oid")
        .insert_many([doc! { "_id": third }, doc! { "_id": first }, doc! { "_id": second }])
        .await
        .unwrap();

    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();
    let page = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"keyset_oid\" WHERE \"_id\" > ? LIMIT 1 OFFSET 0",
            &[Value::Json(Bson::ObjectId(first).into_canonical_extjson())],
        )
        .await
        .unwrap();

    assert_eq!(page.rows.len(), 1);
    assert_eq!(
        page.rows[0][0],
        Value::Json(Bson::ObjectId(second).into_canonical_extjson())
    );
}
