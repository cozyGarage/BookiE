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
async fn filtered_offset_pages_keep_filters_with_and_without_parameters() {
    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .unwrap();
    client
        .database("appdb")
        .collection::<Document>("filtered_offset")
        .insert_many([
            doc! { "_id": 1, "group": "keep", "value": Bson::Null },
            doc! { "_id": 2, "group": "skip" },
            doc! { "_id": 3, "group": "keep", "value": Bson::Null },
            doc! { "_id": 4, "group": "keep", "value": "present" },
        ])
        .await
        .unwrap();

    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();
    let filtered = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"filtered_offset\" WHERE \"group\" = ? LIMIT 1 OFFSET 1",
            &[Value::Text("keep".into())],
        )
        .await
        .unwrap();
    assert_eq!(filtered.rows.len(), 1);
    assert_eq!(filtered.rows[0][0], Value::Int(3));

    let nulls = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"filtered_offset\" WHERE \"value\" IS NULL LIMIT 2 OFFSET 0",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(nulls.rows.len(), 2);
    assert_eq!(nulls.rows[0][0], Value::Int(1));
    assert_eq!(nulls.rows[0][2], Value::Json(serde_json::Value::Null));
    assert_eq!(nulls.rows[1][0], Value::Int(3));
    assert_eq!(nulls.rows[1][2], Value::Json(serde_json::Value::Null));
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
            doc! { "_id": 104, "group": "keep", "region": "east" },
            doc! { "_id": 101, "group": "keep", "region": "east" },
            doc! { "_id": 103, "group": "keep", "region": "east" },
            doc! { "_id": 102, "group": "skip", "region": "east" },
            doc! { "_id": 105, "group": "keep", "region": "west" },
        ])
        .await
        .unwrap();

    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();
    let page = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"filtered_keyset\" WHERE \"group\" = ? AND \"region\" = ? AND \"_id\" > ? LIMIT 2 OFFSET 0",
            &[Value::Text("keep".into()), Value::Text("east".into()), Value::Int(1)],
        )
        .await
        .unwrap();

    assert_eq!(page.rows.len(), 2);
    assert_eq!(
        page.rows[0],
        vec![Value::Int(101), Value::Text("keep".into()), Value::Text("east".into())]
    );
    assert_eq!(
        page.rows[1],
        vec![Value::Int(103), Value::Text("keep".into()), Value::Text("east".into())]
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn null_filtered_keyset_page_excludes_missing_fields() {
    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .unwrap();
    client
        .database("appdb")
        .collection::<Document>("null_filtered_keyset")
        .insert_many([
            doc! { "_id": 1, "value": Bson::Null },
            doc! { "_id": 2 },
            doc! { "_id": 3, "value": Bson::Null },
        ])
        .await
        .unwrap();

    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();
    let page = connection
        .query_params(
            "SELECT * FROM \"appdb\".\"null_filtered_keyset\" WHERE \"value\" IS NULL AND \"_id\" > ? LIMIT 2 OFFSET 0",
            &[Value::Int(1)],
        )
        .await
        .unwrap();

    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0][0], Value::Int(3));
    assert_eq!(page.rows[0][1], Value::Null);
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
