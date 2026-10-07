#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;

#[tokio::test]
#[ignore = "requires docker"]
async fn nested_json_and_mql_filters_match_dotted_fields_and_array_elements() {
    use futures::TryStreamExt;
    use mongodb::bson::{Document, doc};

    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .expect("connect native fixture client");
    let collection = client.database("appdb").collection::<Document>("nested_filters");
    collection
        .insert_many([
            doc! {
                "_id": 1,
                "profile": { "region": "eu", "team": "core" },
                "scores": [2, 8],
                "orders": [{ "sku": "a", "qty": 2 }, { "sku": "b", "qty": 1 }],
                "active": true,
            },
            doc! {
                "_id": 2,
                "profile": { "region": "us", "team": "core" },
                "scores": [8],
                "orders": [{ "sku": "a", "qty": 1 }, { "sku": "b", "qty": 2 }],
                "active": true,
            },
            doc! {
                "_id": 3,
                "profile": { "region": "eu", "team": "ops" },
                "scores": [4],
                "orders": [{ "sku": "b", "qty": 2 }],
                "active": false,
            },
            doc! {
                "_id": 4,
                "profile": { "region": "eu", "team": "core" },
                "scores": [9],
                "orders": [{ "sku": "a", "qty": 2 }],
                "active": true,
            },
        ])
        .await
        .expect("seed nested documents and non-matching controls");

    let connection = MongodbDriver
        .connect(super::opts(&host, port, "appdb"))
        .await
        .expect("connect driver");
    let filter =
        r#"{"profile.region":"eu","profile.team":"core","active":true,"scores":{"$elemMatch":{"$gte":7,"$lt":9}}}"#;
    let queries = [filter.to_string(), format!("db.nested_filters.find({filter})")];
    for query in queries {
        let result = connection.query(&query).await.expect("run nested filter");
        let id_index = result
            .columns
            .iter()
            .position(|column| column.name == "_id")
            .expect("_id metadata");
        let ids = result.rows.iter().map(|row| row[id_index].clone()).collect::<Vec<_>>();
        assert_eq!(ids, vec![Value::Int(1)], "query: {query}");
    }

    // Document 2 has the matching SKU and quantity in different array
    // elements. Dotted predicates can match it; $elemMatch must not.
    let filter = r#"{"orders":{"$elemMatch":{"sku":"a","qty":{"$gte":2}}}}"#;
    let native_filter = doc! { "orders": { "$elemMatch": { "sku": "a", "qty": { "$gte": 2 } } } };
    let native_matches = collection
        .find(native_filter)
        .sort(doc! { "_id": 1 })
        .await
        .expect("run native nested-array oracle")
        .try_collect::<Vec<Document>>()
        .await
        .expect("collect native nested-array oracle");
    let native_ids = native_matches
        .iter()
        .map(|document| i64::from(document.get_i32("_id").expect("native id")))
        .collect::<Vec<_>>();
    assert_eq!(native_ids, vec![1, 4]);

    for query in [filter.to_string(), format!("db.nested_filters.find({filter})")] {
        let result = connection.query(&query).await.expect("run nested-array filter");
        let id_index = result
            .columns
            .iter()
            .position(|column| column.name == "_id")
            .expect("_id metadata");
        let ids = result
            .rows
            .iter()
            .map(|row| row[id_index].clone())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![Value::Int(1), Value::Int(4)], "query: {query}");
    }
}
