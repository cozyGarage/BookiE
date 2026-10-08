use drivers_mongodb::MongodbDriver;
use mongodb::bson::{Document, doc};
use std::time::Instant;
use tablepro_core::DatabaseDriver;

#[tokio::test]
#[ignore = "requires docker; diagnostic timing only"]
async fn value_contract_mongodb_census_scan_cost_profile() {
    let (_container, host, port) = super::start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .expect("connect direct BSON fixture client");
    let database = client.database("appdb");
    let connection = MongodbDriver.connect(super::opts(&host, port, "appdb")).await.unwrap();

    for size in [1_000_i32, 10_000_i32, 100_000_i32, 1_000_000_i32] {
        let collection_name = format!("census_cost_{size}");
        let collection = database.collection::<Document>(&collection_name);
        let documents = (0..size)
            .map(|id| doc! { "_id": id, "value": format!("row-{id}") })
            .collect::<Vec<_>>();
        collection
            .insert_many(documents)
            .await
            .expect("seed census cost fixture");

        let mut census_samples = Vec::with_capacity(5);
        for _ in 0..5 {
            let started = Instant::now();
            let columns = connection.fetch_columns(None, &collection_name).await.unwrap();
            census_samples.push(started.elapsed());
            assert!(columns.iter().any(|column| column.name == "value"));
        }
        let mut page_samples = Vec::with_capacity(5);
        for _ in 0..5 {
            let started = Instant::now();
            let page = connection.fetch_rows(None, &collection_name, 0, 50).await.unwrap();
            page_samples.push(started.elapsed());
            assert_eq!(page.rows.len(), 50);
        }
        census_samples.sort_unstable();
        page_samples.sort_unstable();
        println!(
            "census rows={size} samples_us={:?} median_us={}; warm_page_rows=50 samples_us={:?} median_us={}",
            census_samples
                .iter()
                .map(|sample| sample.as_micros())
                .collect::<Vec<_>>(),
            census_samples[2].as_micros(),
            page_samples.iter().map(|sample| sample.as_micros()).collect::<Vec<_>>(),
            page_samples[2].as_micros(),
        );
    }
}
