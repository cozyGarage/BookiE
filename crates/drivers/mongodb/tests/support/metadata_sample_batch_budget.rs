use futures::TryStreamExt;
use mongodb::bson::{Binary, Bson, doc, spec::BinarySubtype};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use bookie_core::DatabaseDriver;

use super::{MongodbDriver, opts, start_mongo};

#[tokio::test]
#[ignore = "requires docker"]
async fn a_wide_metadata_sample_bounds_cursor_batches() {
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
    let started = Instant::now();
    let columns = connection
        .fetch_columns(None, "wide_metadata_sample")
        .await
        .expect("fetch metadata for wide documents");
    let local_elapsed = started.elapsed();
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

    let (proxy_port, proxy_task) = start_delayed_proxy(&host, port, Duration::from_millis(20)).await;
    let delayed_connection = MongodbDriver
        .connect(opts("127.0.0.1", proxy_port, "appdb"))
        .await
        .expect("connect through latency proxy");
    let started = Instant::now();
    delayed_connection
        .fetch_columns(None, "wide_metadata_sample")
        .await
        .expect("fetch metadata through latency proxy");
    let delayed_elapsed = started.elapsed();
    proxy_task.abort();

    eprintln!(
        "MongoDB metadata sample: batch size 16; local={local_elapsed:?}; 20ms response delay={delayed_elapsed:?}"
    );
    assert_profiled_sample_batch_size(&database, 4, 16).await;
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

async fn assert_profiled_sample_batch_size(
    database: &mongodb::Database,
    expected_samples: usize,
    expected_batch_size: i32,
) {
    let profile = database.collection::<mongodb::bson::Document>("system.profile");
    let mut entries = profile
        .find(doc! { "$or": [
            { "command.find": "wide_metadata_sample", "command.limit": 128 },
            { "command.getMore": { "$exists": true }, "command.collection": "wide_metadata_sample" },
        ] })
        .await
        .expect("read profiled metadata sample");
    let (mut finds, mut get_mores) = (0, 0);
    while let Some(entry) = entries.try_next().await.expect("read profile entry") {
        let command = entry.get_document("command").expect("profile command document");
        assert_eq!(command.get("batchSize"), Some(&Bson::Int32(expected_batch_size)));
        if command.contains_key("find") {
            finds += 1;
        } else {
            get_mores += 1;
        }
    }
    assert_eq!(finds, expected_samples);
    assert_eq!(get_mores, expected_samples * 7);
}

async fn start_delayed_proxy(host: &str, port: u16, delay: Duration) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind latency proxy");
    let proxy_port = listener.local_addr().expect("proxy address").port();
    let target = format!("{host}:{port}");
    let task = tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let target = target.clone();
            tokio::spawn(async move {
                let Ok(server) = TcpStream::connect(target).await else {
                    return;
                };
                let (mut client_read, mut client_write) = client.into_split();
                let (mut server_read, mut server_write) = server.into_split();
                let requests = copy_mongo_messages(&mut client_read, &mut server_write, Duration::ZERO);
                let responses = copy_mongo_messages(&mut server_read, &mut client_write, delay);
                tokio::select! {
                    _ = requests => {},
                    _ = responses => {},
                }
            });
        }
    });
    (proxy_port, task)
}

async fn copy_mongo_messages<R, W>(reader: &mut R, writer: &mut W, delay: Duration)
where
    R: AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    loop {
        let mut header = [0; 4];
        if reader.read_exact(&mut header).await.is_err() {
            return;
        }
        let length = i32::from_le_bytes(header) as usize;
        if !(16..=48 * 1024 * 1024).contains(&length) {
            return;
        }
        let mut message = vec![0; length];
        message[..4].copy_from_slice(&header);
        if reader.read_exact(&mut message[4..]).await.is_err() {
            return;
        }
        tokio::time::sleep(delay).await;
        if writer.write_all(&message).await.is_err() {
            return;
        }
    }
}
