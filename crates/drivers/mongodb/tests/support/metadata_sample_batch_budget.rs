use futures::TryStreamExt;
use mongodb::bson::{Binary, Bson, doc, spec::BinarySubtype};

use bookie_core::DatabaseDriver;

use super::{MongodbDriver, opts, start_mongo};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_wide_metadata_sample_requests_one_document_per_cursor_batch() {
    let (_container, host, port) = start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .expect("connect profiler fixture client");
    let database = client.database("appdb");
    database
        .run_command(doc! { "profile": 2 })
        .await
        .expect("enable command profiling");
    let collection = database.collection::<mongodb::bson::Document>("wide_metadata_sample");
    seed_wide_documents(&collection).await;

    let connection = MongodbDriver
        .connect(opts(&host, port, "appdb"))
        .await
        .expect("connect BookiE");
    let columns = connection
        .fetch_columns(None, "wide_metadata_sample")
        .await
        .expect("fetch metadata for wide documents");
    assert!(columns.iter().any(|column| column.name == "payload"));
    let page = connection
        .fetch_rows(None, "wide_metadata_sample", 0, 1)
        .await
        .expect("fetch one row through the metadata sampler");
    assert_eq!(page.rows.len(), 1);
    let query = connection
        .query("db.wide_metadata_sample.find({}).limit(1)")
        .await
        .expect("run a one-row query through the metadata sampler");
    assert_eq!(query.rows.len(), 1);

    assert_profiled_sample_batch_size(&database, 3).await;
}

async fn seed_wide_documents(collection: &mongodb::Collection<mongodb::bson::Document>) {
    let payload = Binary {
        subtype: BinarySubtype::Generic,
        bytes: vec![0x5a; 256 * 1024],
    };
    let documents = (0..128)
        .map(|id| doc! { "_id": id, "payload": payload.clone() })
        .collect::<Vec<_>>();
    collection
        .insert_many(documents)
        .await
        .expect("seed wide sample documents");
}

async fn assert_profiled_sample_batch_size(database: &mongodb::Database, expected_samples: usize) {
    let profile = database.collection::<mongodb::bson::Document>("system.profile");
    let mut entries = profile
        .find(doc! {
            "command.find": "wide_metadata_sample",
            "command.limit": 128,
        })
        .await
        .expect("read profiled metadata sample");
    let mut observed = 0;
    while let Some(entry) = entries.try_next().await.expect("read profile entry") {
        let command = entry.get_document("command").expect("profile command document");
        assert_eq!(command.get("batchSize"), Some(&Bson::Int32(1)));
        observed += 1;
    }
    assert_eq!(observed, expected_samples);
}
