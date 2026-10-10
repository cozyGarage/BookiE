use drivers_mongodb::MongodbDriver;
use mongodb::bson::{Binary, Bson, Document, doc, oid::ObjectId, spec::BinarySubtype};
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
async fn filtered_keyset_page_keeps_the_filter_and_cursor() {
    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .unwrap();
    client
        .database("appdb")
        .collection::<Document>("filtered_keyset")
        .insert_many([
            doc! { "_id": 1, "group": "keep" },
            doc! { "_id": 2, "group": "skip" },
            doc! { "_id": 3, "group": "keep" },
            doc! { "_id": 4, "group": "keep" },
        ])
        .await
        .unwrap();

    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();
    let page = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"filtered_keyset\" WHERE \"group\" = ? AND \"_id\" > ? LIMIT 2 OFFSET 0",
            &[Value::Text("keep".into()), Value::Int(1)],
        )
        .await
        .unwrap();

    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.rows[0], vec![Value::Int(3), Value::Text("keep".into())]);
    assert_eq!(page.rows[1], vec![Value::Int(4), Value::Text("keep".into())]);
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

#[tokio::test]
#[ignore = "requires docker"]
async fn keyset_page_query_keeps_mixed_bson_id_types_in_sort_order() {
    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .unwrap();
    let object_id = ObjectId::parse_str("000000000000000000000001").unwrap();
    client
        .database("appdb")
        .collection::<Document>("keyset_mixed_ids")
        .insert_many([
            doc! { "_id": 1, "value": "number" },
            doc! { "_id": "middle", "value": "string" },
            doc! { "_id": object_id, "value": "object id" },
        ])
        .await
        .unwrap();
    let explain = client
        .database("appdb")
        .run_command(doc! {
            "explain": {
                "find": "keyset_mixed_ids",
                "filter": { "$expr": { "$gt": ["$_id", { "$literal": 1 }] } },
                "sort": { "_id": 1 },
                "limit": 1,
            },
            "verbosity": "queryPlanner",
        })
        .await
        .unwrap();
    assert!(explain.to_string().contains("IXSCAN"), "{explain:?}");

    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();
    let page = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"keyset_mixed_ids\" WHERE \"_id\" > ? LIMIT 1 OFFSET 0",
            &[Value::Int(1)],
        )
        .await
        .unwrap();

    assert_eq!(page.rows.len(), 1);
    assert_eq!(
        page.rows[0][0],
        Value::Json(Bson::String("middle".into()).into_canonical_extjson())
    );
    let next_page = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"keyset_mixed_ids\" WHERE \"_id\" > ? LIMIT 1 OFFSET 0",
            &[Value::Json(Bson::String("middle".into()).into_canonical_extjson())],
        )
        .await
        .unwrap();
    assert_eq!(next_page.rows.len(), 1);
    assert_eq!(
        next_page.rows[0][0],
        Value::Json(Bson::ObjectId(object_id).into_canonical_extjson())
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn keyset_page_query_binds_binary_uuid_cursor_values() {
    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .unwrap();
    let first = Binary {
        subtype: BinarySubtype::Uuid,
        bytes: vec![0; 16],
    };
    let second = Binary {
        subtype: BinarySubtype::Uuid,
        bytes: vec![1; 16],
    };
    let third = Binary {
        subtype: BinarySubtype::Uuid,
        bytes: vec![2; 16],
    };
    client
        .database("appdb")
        .collection::<Document>("keyset_uuid")
        .insert_many([
            doc! { "_id": Bson::Binary(third) },
            doc! { "_id": Bson::Binary(first.clone()) },
            doc! { "_id": Bson::Binary(second.clone()) },
        ])
        .await
        .unwrap();

    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();
    let page = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"keyset_uuid\" WHERE \"_id\" > ? LIMIT 1 OFFSET 0",
            &[Value::Json(Bson::Binary(first).into_canonical_extjson())],
        )
        .await
        .unwrap();

    assert_eq!(page.rows.len(), 1);
    assert_eq!(
        page.rows[0][0],
        Value::Json(Bson::Binary(second).into_canonical_extjson())
    );
}
