use super::{Value, parse_input_for_driver};

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_mongodb_nested_document_edit_preserves_extended_bson_and_row_identity() {
    use mongodb::bson::{Bson, Decimal128, Document, doc, oid::ObjectId};
    use tablepro_core::{ConnectOptions, DatabaseDriver};
    use testcontainers::ImageExt;
    use testcontainers_modules::mongo::Mongo;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    let container = Mongo::default().with_tag("7").start().await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(27017).await.unwrap();
    let native = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .unwrap();
    let collection = native.database("appdb").collection::<Document>("nested_grid_edits");
    let row_id = ObjectId::parse_str("0123456789abcdef01234567").unwrap();
    let sibling_id = ObjectId::parse_str("fedcba987654321001234567").unwrap();
    let before = doc! {
        "profile": { "name": "before", "visits": Bson::Int64(9_007_199_254_740_993) },
        "amount": "1.2300",
        "labels": ["keep", "shape"],
    };
    collection
        .insert_many([
            doc! { "_id": row_id, "payload": before.clone(), "outside": "target" },
            doc! { "_id": sibling_id, "payload": { "untouched": true }, "outside": "sibling" },
        ])
        .await
        .unwrap();

    let connection = drivers_mongodb::MongodbDriver
        .connect(ConnectOptions {
            host,
            port,
            database: "appdb".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let result = connection.query("db.nested_grid_edits.find({})").await.unwrap();
    let id_index = result.columns.iter().position(|column| column.name == "_id").unwrap();
    let payload_index = result
        .columns
        .iter()
        .position(|column| column.name == "payload")
        .unwrap();
    assert_eq!(result.columns[payload_index].data_type, "object");
    let row = result
        .rows
        .iter()
        .find(|row| row[id_index] == Value::Json(serde_json::json!({ "$oid": row_id.to_hex() })))
        .expect("target document is present");
    let Value::Json(displayed) = &row[payload_index] else {
        panic!(
            "nested BSON document must display as canonical Extended JSON: {:?}",
            row[payload_index]
        );
    };
    assert_eq!(*displayed, Bson::Document(before).into_canonical_extjson());

    let input = r#"{"profile":{"name":"after","visits":{"$numberLong":"9007199254740993"}},"amount":{"$numberDecimal":"12.3400"},"owner":{"$oid":"00112233445566778899aabb"},"labels":["edited","shape"]}"#;
    let edited = parse_input_for_driver(input, Some(&result.columns[payload_index]), "mongodb").unwrap();
    let update = tablepro_core::sql_dialect::build_keyed_update(
        "mongodb",
        Some("appdb"),
        "nested_grid_edits",
        &result.columns,
        &[(payload_index, edited)],
        &[row[id_index].clone()],
    )
    .unwrap();
    assert_eq!(connection.execute_in_transaction(&[update]).await.unwrap(), vec![1]);

    let persisted = collection
        .find_one(doc! { "_id": row_id })
        .await
        .unwrap()
        .expect("target document remains present");
    assert_eq!(persisted.get("_id"), Some(&Bson::ObjectId(row_id)));
    assert_eq!(persisted.get("outside"), Some(&Bson::String("target".into())));
    let payload = persisted.get_document("payload").unwrap();
    assert_eq!(
        payload.get_document("profile").unwrap().get_str("name").unwrap(),
        "after"
    );
    assert_eq!(
        payload.get_document("profile").unwrap().get_i64("visits").unwrap(),
        9_007_199_254_740_993
    );
    assert_eq!(
        payload.get("amount"),
        Some(&Bson::Decimal128("12.3400".parse::<Decimal128>().unwrap()))
    );
    assert_eq!(
        payload.get("owner"),
        Some(&Bson::ObjectId(
            ObjectId::parse_str("00112233445566778899aabb").unwrap()
        ))
    );

    let sibling = collection
        .find_one(doc! { "_id": sibling_id })
        .await
        .unwrap()
        .expect("sibling document remains present");
    assert_eq!(sibling.get("_id"), Some(&Bson::ObjectId(sibling_id)));
    assert_eq!(
        sibling.get("payload"),
        Some(&Bson::Document(doc! { "untouched": true }))
    );
    assert_eq!(sibling.get("outside"), Some(&Bson::String("sibling".into())));
}
