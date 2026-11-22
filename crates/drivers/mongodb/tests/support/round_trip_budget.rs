use futures::TryStreamExt;
use mongodb::bson::{Document, doc};

use super::{MongodbDriver, opts, start_mongo};
use bookie_core::{DatabaseDriver, Value};

const SIMPLE_FIND_COMMAND_BUDGET: usize = 2;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_simple_find_stays_within_its_server_command_budget() {
    let (_container, host, port) = start_mongo().await;
    let observer = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .expect("connect profiler observer");
    let database = observer.database("appdb");
    let collection = database.collection::<Document>("round_trip_budget_docs");
    collection
        .insert_one(doc! { "value": 42_i32 })
        .await
        .expect("seed measured collection");

    let connection = MongodbDriver
        .connect(opts(&host, port, "appdb"))
        .await
        .expect("connect BookiE");
    connection.list_tables().await.expect("warm BookiE connection");
    database
        .run_command(doc! { "profile": 2 })
        .await
        .expect("enable profiling");

    let result = connection
        .query("db.round_trip_budget_docs.find({}).limit(1)")
        .await
        .expect("run measured find");
    assert_eq!(result.rows.len(), 1);
    assert!(result.rows[0].contains(&Value::Int(42)));

    let profile = database.collection::<Document>("system.profile");
    let entries = profile
        .find(doc! { "ns": "appdb.round_trip_budget_docs" })
        .await
        .expect("read server command log")
        .try_collect::<Vec<_>>()
        .await
        .expect("collect server command log");
    let commands = entries.iter().map(|entry| format!("{entry:#?}")).collect::<Vec<_>>();
    assert!(
        commands.len() <= SIMPLE_FIND_COMMAND_BUDGET,
        "simple find used {} server commands (budget {SIMPLE_FIND_COMMAND_BUDGET}):\n{}",
        commands.len(),
        commands.join("\n")
    );
    assert!(
        commands[0].contains("round_trip_budget_docs"),
        "server trace omitted measured find: {commands:#?}"
    );
}
