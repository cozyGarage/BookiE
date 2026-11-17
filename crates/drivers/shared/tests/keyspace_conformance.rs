use bookie_core::{ConnectOptions, Connection, DataModel, DatabaseDriver, QueryResult, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    String,
    Hash,
    List,
    Set,
    SortedSet,
    Stream,
}

impl KeyKind {
    pub const ALL: [KeyKind; 6] = [
        KeyKind::String,
        KeyKind::Hash,
        KeyKind::List,
        KeyKind::Set,
        KeyKind::SortedSet,
        KeyKind::Stream,
    ];

    fn has_member_count(self) -> bool {
        matches!(self, KeyKind::Hash | KeyKind::List | KeyKind::Set | KeyKind::SortedSet)
    }
}

pub const COLLECTION_MEMBERS: i64 = 2;

#[allow(async_fn_in_trait)]
pub trait KeyspaceEngine {
    type Driver: DatabaseDriver;
    type Scratch;
    const DRIVER_ENFORCES_READ_ONLY: bool = true;
    const KEY_COLUMN: &'static str = "Key";
    const TYPE_COLUMN: &'static str = "Type";
    const TTL_COLUMN: &'static str = "TTL";
    const MEMBERS_COLUMN: &'static str = "Cardinality";
    const VALUE_COLUMN: &'static str = "Value";
    const NO_EXPIRY_TTL: i64 = -1;
    fn driver() -> Self::Driver;
    async fn scratch() -> Self::Scratch;
    fn options(scratch: &Self::Scratch, read_only: bool) -> ConnectOptions;
    fn browse_target(scratch: &Self::Scratch) -> String;
    fn full_value_query(scratch: &Self::Scratch) -> String;
    fn type_label(kind: KeyKind) -> &'static str;
    fn write_statements() -> Vec<String>;
    async fn seed_string(scratch: &Self::Scratch, key: &[u8], value: &[u8]);
    async fn seed_collection(scratch: &Self::Scratch, key: &[u8], kind: KeyKind);
    async fn expire_in(scratch: &Self::Scratch, key: &[u8], seconds: u32);
}

async fn open<E: KeyspaceEngine>(scratch: &E::Scratch, read_only: bool) -> Box<dyn Connection> {
    E::driver()
        .connect(E::options(scratch, read_only))
        .await
        .expect("connect")
}

fn column_index<E: KeyspaceEngine>(result: &QueryResult, name: &str) -> usize {
    result
        .columns
        .iter()
        .position(|column| column.name == name)
        .unwrap_or_else(|| panic!("browse result has a {name} column"))
}

async fn browse_page<E: KeyspaceEngine>(
    connection: &dyn Connection,
    scratch: &E::Scratch,
    offset: u64,
    limit: u64,
) -> (QueryResult, Vec<Vec<Value>>) {
    let page = connection
        .fetch_rows(None, &E::browse_target(scratch), offset, limit)
        .await
        .expect("browse page");
    let rows = page.rows.iter().map(|row| row.to_vec()).collect();
    (page, rows)
}

async fn browse_everything<E: KeyspaceEngine>(connection: &dyn Connection, scratch: &E::Scratch) -> Vec<Vec<Value>> {
    browse_page::<E>(connection, scratch, 0, 1_000).await.1
}

fn text_or_bytes(bytes: &[u8]) -> Value {
    match std::str::from_utf8(bytes) {
        Ok(text) => Value::Text(text.to_string()),
        Err(_) => Value::Bytes(bytes.to_vec()),
    }
}

fn key_text(index: usize) -> String {
    format!("conf:key:{index:03}")
}

fn row_for<'a>(rows: &'a [Vec<Value>], key_index: usize, key: &str) -> &'a [Value] {
    rows.iter()
        .find(|row| row[key_index] == Value::Text(key.into()))
        .unwrap_or_else(|| panic!("{key} is listed"))
}

fn value_corpus() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("conf:empty", Vec::new()),
        ("conf:nul", b"before\0after".to_vec()),
        ("conf:binary", vec![0xff, 0xfe, 0x00, 0x80, 0x41]),
        (
            "conf:unicode",
            "h\u{e9}llo \u{4e16}\u{754c} \u{1f680}".as_bytes().to_vec(),
        ),
        ("conf:large", (0..200_000_u32).map(|n| b'a' + (n % 26) as u8).collect()),
    ]
}

pub async fn declared_capabilities_match_the_suite<E: KeyspaceEngine>() {
    let capabilities = E::driver().capabilities();
    assert_eq!(capabilities.data_model, DataModel::KeyValue);
    assert!(!capabilities.is_file_based);
    assert!(!E::driver().id().is_empty());
}

pub async fn exact_values_round_trip_through_the_full_value_read<E: KeyspaceEngine>() {
    let scratch = E::scratch().await;
    let corpus = value_corpus();
    for (key, value) in &corpus {
        E::seed_string(&scratch, key.as_bytes(), value).await;
    }
    let connection = open::<E>(&scratch, true).await;
    let query = E::full_value_query(&scratch);
    for (key, value) in &corpus {
        let found = connection
            .query_params(&query, &[Value::Text((*key).into())])
            .await
            .expect("full value read");
        let rows: Vec<Vec<Value>> = found.rows.iter().map(|row| row.to_vec()).collect();
        assert_eq!(rows, vec![vec![text_or_bytes(value)]], "{key}");
    }
}

pub async fn browse_previews_keep_small_values_exact_and_never_pass_off_a_large_value_as_whole<E: KeyspaceEngine>() {
    let scratch = E::scratch().await;
    let corpus = value_corpus();
    for (key, value) in &corpus {
        E::seed_string(&scratch, key.as_bytes(), value).await;
    }
    let connection = open::<E>(&scratch, true).await;
    let (page, rows) = browse_page::<E>(connection.as_ref(), &scratch, 0, 100).await;
    let key_index = column_index::<E>(&page, E::KEY_COLUMN);
    let value_index = column_index::<E>(&page, E::VALUE_COLUMN);
    for (key, value) in &corpus {
        let preview = &row_for(&rows, key_index, key)[value_index];
        if value.len() < 4_096 {
            assert_eq!(preview, &text_or_bytes(value), "{key}");
        } else {
            assert_ne!(
                preview,
                &text_or_bytes(value),
                "{key} preview must not claim to be the whole value"
            );
        }
    }
}

pub async fn key_types_are_reported_and_collections_show_counts_not_members<E: KeyspaceEngine>() {
    let scratch = E::scratch().await;
    for (index, kind) in KeyKind::ALL.into_iter().enumerate() {
        match kind {
            KeyKind::String => E::seed_string(&scratch, key_text(index).as_bytes(), b"hello").await,
            _ => E::seed_collection(&scratch, key_text(index).as_bytes(), kind).await,
        }
    }
    let connection = open::<E>(&scratch, true).await;
    let (page, rows) = browse_page::<E>(connection.as_ref(), &scratch, 0, 100).await;
    assert_eq!(rows.len(), KeyKind::ALL.len());
    let key_index = column_index::<E>(&page, E::KEY_COLUMN);
    let type_index = column_index::<E>(&page, E::TYPE_COLUMN);
    let members_index = column_index::<E>(&page, E::MEMBERS_COLUMN);
    let value_index = column_index::<E>(&page, E::VALUE_COLUMN);
    for (index, kind) in KeyKind::ALL.into_iter().enumerate() {
        let row = row_for(&rows, key_index, &key_text(index));
        assert_eq!(row[type_index], Value::Text(E::type_label(kind).into()), "{kind:?}");
        if kind.has_member_count() {
            assert_eq!(row[members_index], Value::Int(COLLECTION_MEMBERS), "{kind:?}");
        } else {
            assert_eq!(row[members_index], Value::Null, "{kind:?}");
        }
        if kind != KeyKind::String {
            assert!(
                matches!(row[value_index], Value::Undecodable(_)),
                "{kind:?} shows no members"
            );
        }
    }
}

pub async fn browse_pages_cover_every_key_once_and_the_last_page_is_short<E: KeyspaceEngine>() {
    const KEYS: usize = 25;
    const PAGE: u64 = 10;
    let scratch = E::scratch().await;
    for index in 0..KEYS {
        E::seed_string(&scratch, key_text(index).as_bytes(), b"v").await;
    }
    let connection = open::<E>(&scratch, true).await;
    let mut seen = Vec::new();
    for (offset, expected_len) in [(0_u64, 10_usize), (10, 10), (20, 5), (30, 0)] {
        let (page, rows) = browse_page::<E>(connection.as_ref(), &scratch, offset, PAGE).await;
        assert_eq!(rows.len(), expected_len, "offset {offset}");
        let key_index = column_index::<E>(&page, E::KEY_COLUMN);
        seen.extend(rows.into_iter().map(|row| row[key_index].clone()));
    }
    seen.sort_by_key(|key| format!("{key:?}"));
    let expected: Vec<Value> = (0..KEYS).map(|index| Value::Text(key_text(index))).collect();
    assert_eq!(seen, expected);
}

pub async fn read_only_connections_refuse_every_write_and_keep_the_keys<E: KeyspaceEngine>() {
    let scratch = E::scratch().await;
    for index in 0..3 {
        E::seed_string(&scratch, key_text(index).as_bytes(), b"original").await;
    }
    let reader = open::<E>(&scratch, true).await;
    let before = browse_everything::<E>(reader.as_ref(), &scratch).await;
    assert_eq!(before.len(), 3, "a read-only connection still reads");
    if !E::DRIVER_ENFORCES_READ_ONLY {
        return;
    }
    for statement in E::write_statements() {
        assert!(reader.execute(&statement).await.is_err(), "{statement} must be refused");
    }
    let batch: Vec<(String, Vec<Value>)> = E::write_statements().into_iter().map(|sql| (sql, Vec::new())).collect();
    assert!(reader.execute_in_transaction(&batch).await.is_err());
    assert_eq!(browse_everything::<E>(reader.as_ref(), &scratch).await, before);
}

pub async fn ttl_is_reported_and_keys_without_expiry_stay_persistent<E: KeyspaceEngine>() {
    let scratch = E::scratch().await;
    E::seed_string(&scratch, b"conf:persistent", b"v").await;
    E::seed_collection(&scratch, b"conf:persistent-list", KeyKind::List).await;
    E::seed_string(&scratch, b"conf:expiring", b"v").await;
    E::expire_in(&scratch, b"conf:expiring", 1_000).await;
    let connection = open::<E>(&scratch, true).await;
    for _ in 0..2 {
        let (page, rows) = browse_page::<E>(connection.as_ref(), &scratch, 0, 100).await;
        let key_index = column_index::<E>(&page, E::KEY_COLUMN);
        let ttl_index = column_index::<E>(&page, E::TTL_COLUMN);
        for key in ["conf:persistent", "conf:persistent-list"] {
            assert_eq!(
                row_for(&rows, key_index, key)[ttl_index],
                Value::Int(E::NO_EXPIRY_TTL),
                "{key}"
            );
        }
        let Value::Int(ttl) = row_for(&rows, key_index, "conf:expiring")[ttl_index] else {
            panic!("an expiring key reports an integer TTL");
        };
        assert!((1..=1_000).contains(&ttl), "ttl {ttl}");
    }
}

#[allow(dead_code)]
pub async fn run_all<E: KeyspaceEngine>() {
    declared_capabilities_match_the_suite::<E>().await;
    exact_values_round_trip_through_the_full_value_read::<E>().await;
    browse_previews_keep_small_values_exact_and_never_pass_off_a_large_value_as_whole::<E>().await;
    key_types_are_reported_and_collections_show_counts_not_members::<E>().await;
    browse_pages_cover_every_key_once_and_the_last_page_is_short::<E>().await;
    read_only_connections_refuse_every_write_and_keep_the_keys::<E>().await;
    ttl_is_reported_and_keys_without_expiry_stay_persistent::<E>().await;
}

#[macro_export]
macro_rules! keyspace_conformance_suite {
    ($engine:ty) => {
        #[tokio::test]
        async fn keyspace_declared_capabilities_match_the_suite() {
            keyspace_conformance::declared_capabilities_match_the_suite::<$engine>().await;
        }
        #[tokio::test]
        async fn keyspace_exact_values_round_trip_through_the_full_value_read() {
            keyspace_conformance::exact_values_round_trip_through_the_full_value_read::<$engine>().await;
        }
        #[tokio::test]
        async fn keyspace_browse_previews_keep_small_values_exact() {
            keyspace_conformance::browse_previews_keep_small_values_exact_and_never_pass_off_a_large_value_as_whole::<
                $engine,
            >()
            .await;
        }
        #[tokio::test]
        async fn keyspace_key_types_are_reported_and_collections_show_counts_not_members() {
            keyspace_conformance::key_types_are_reported_and_collections_show_counts_not_members::<$engine>().await;
        }
        #[tokio::test]
        async fn keyspace_browse_pages_cover_every_key_once_and_the_last_page_is_short() {
            keyspace_conformance::browse_pages_cover_every_key_once_and_the_last_page_is_short::<$engine>().await;
        }
        #[tokio::test]
        async fn keyspace_read_only_connections_refuse_every_write_and_keep_the_keys() {
            keyspace_conformance::read_only_connections_refuse_every_write_and_keep_the_keys::<$engine>().await;
        }
        #[tokio::test]
        async fn keyspace_ttl_is_reported_and_keys_without_expiry_stay_persistent() {
            keyspace_conformance::ttl_is_reported_and_keys_without_expiry_stay_persistent::<$engine>().await;
        }
    };
}
