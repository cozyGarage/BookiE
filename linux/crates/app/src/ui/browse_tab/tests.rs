use super::grid_render::column_layout_matches;
use super::{BrowsePageRequest, PageRequestTracker, RowCountRequestTracker, columns_for_browse_page};
use tablepro_core::{ColumnInfo, QueryResult, Value};
use uuid::Uuid;

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
    }
}

#[test]
fn only_the_latest_browse_page_request_is_accepted() {
    let tracker = PageRequestTracker::default();
    let older = tracker.begin(0);
    let newer = tracker.begin(0);

    assert!(!tracker.accepts(older, 0));
    assert!(tracker.accepts(newer, 0));
}

#[test]
fn only_the_latest_row_count_request_is_accepted() {
    let tracker = RowCountRequestTracker::default();
    let older = tracker.begin();
    let newer = tracker.begin();

    assert!(!tracker.accepts(older));
    assert!(tracker.accepts(newer));
}

#[test]
fn browse_page_response_must_match_the_current_offset() {
    let tracker = PageRequestTracker::default();
    let request = tracker.begin(100);
    let same_id_wrong_offset = BrowsePageRequest {
        id: request.id,
        offset: 100,
    };

    assert!(!tracker.accepts(same_id_wrong_offset, 200));
    assert_ne!(request.id, Uuid::nil());
}

#[test]
fn mongodb_page_schema_updates_late_fields_and_mixed_types_before_grid_editing() {
    let loaded = vec![column("_id", "ObjectId", true), column("value", "string", false)];
    let page_columns = vec![
        column("_id", "ObjectId", true),
        column("value", "mixed", false),
        column("late_field", "Decimal128", false),
    ];
    let page = QueryResult {
        columns: page_columns.clone(),
        rows: vec![vec![
            Value::Uuid(Uuid::new_v4()),
            Value::Json(serde_json::json!({"$numberDecimal": "1.25"})),
            Value::Text("late".into()),
        ]],
        truncated: false,
    };

    let effective = columns_for_browse_page("mongodb", &loaded, Some(&page));
    assert_eq!(effective, page_columns);
    assert_eq!(effective[1].data_type, "mixed");
    assert_eq!(effective[2].name, "late_field");
    assert!(
        !column_layout_matches(&loaded, &effective),
        "new page metadata must rebuild cached factories and editability"
    );
    let text_value = Value::Text("ordinary text on the mixed page".into());
    assert!(crate::ui::grid::cell_allows_inline_edit(&loaded[1], &text_value));
    assert!(
        !crate::ui::grid::cell_allows_inline_edit(&effective[1], &text_value),
        "page-discovered mixed BSON types must make the same text cell read-only"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_mongodb_collection_wide_mixed_metadata_refuses_edit() {
    use tablepro_core::OperationControl;

    let (_container, connection, collection, decimal) = mongodb_late_mixed_page_fixture().await;
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    let loaded_columns = connection
        .fetch_columns_controlled(None, "page_mixed_edit_contract", &control)
        .await
        .unwrap();
    let loaded_value_index = loaded_columns.iter().position(|column| column.name == "value").unwrap();
    assert_eq!(loaded_columns[loaded_value_index].data_type, "mixed");

    let page = connection
        .fetch_rows_controlled(None, "page_mixed_edit_contract", 0, 1, &control)
        .await
        .unwrap();
    let effective_columns = columns_for_browse_page("mongodb", &loaded_columns, Some(&page));
    let value_index = effective_columns
        .iter()
        .position(|column| column.name == "value")
        .unwrap();
    let id_index = effective_columns
        .iter()
        .position(|column| column.name == "_id")
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0][id_index], Value::Int(0));
    assert_eq!(effective_columns[value_index].data_type, "mixed");
    assert!(column_layout_matches(&loaded_columns, &effective_columns));
    assert!(
        !crate::ui::grid::cell_allows_inline_edit(&effective_columns[value_index], &page.rows[0][value_index]),
        "a conflict beyond the returned page must make its first-page cell read-only"
    );

    let later_page = connection
        .fetch_rows_controlled(None, "page_mixed_edit_contract", 50, 1, &control)
        .await
        .unwrap();
    let later_columns = columns_for_browse_page("mongodb", &loaded_columns, Some(&later_page));
    let later_value_index = later_columns.iter().position(|column| column.name == "value").unwrap();
    let later_id_index = later_columns.iter().position(|column| column.name == "_id").unwrap();
    assert_eq!(later_page.rows[0][later_id_index], Value::Int(50));
    assert_eq!(
        later_page.rows[0][later_value_index],
        Value::Json(serde_json::json!({"$numberDecimal": decimal.to_string()}))
    );
    assert_eq!(later_columns[later_value_index].data_type, "mixed");
    assert!(column_layout_matches(&loaded_columns, &later_columns));
    assert!(
        !crate::ui::grid::cell_allows_inline_edit(
            &later_columns[later_value_index],
            &later_page.rows[0][later_value_index]
        ),
        "the later Decimal128 page must remain read-only under the collection-wide schema"
    );

    let persisted = collection
        .find_one(mongodb::bson::doc! { "_id": 50 })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.get("_id"), Some(&mongodb::bson::Bson::Int32(50)));
    assert_eq!(persisted.get("value"), Some(&mongodb::bson::Bson::Decimal128(decimal)));
}

async fn mongodb_late_mixed_page_fixture() -> (
    testcontainers::ContainerAsync<testcontainers_modules::mongo::Mongo>,
    Box<dyn tablepro_core::Connection>,
    mongodb::Collection<mongodb::bson::Document>,
    mongodb::bson::Decimal128,
) {
    use mongodb::bson::{Decimal128, doc};
    use tablepro_core::DatabaseDriver;
    use testcontainers::ImageExt;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::mongo::Mongo;

    let container = Mongo::default().with_tag("7").start().await.unwrap();
    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(27017).await.unwrap();
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .unwrap();
    let collection = client
        .database("appdb")
        .collection::<mongodb::bson::Document>("page_mixed_edit_contract");
    let decimal_text = "12345678901234567890.1234567890123";
    let decimal = decimal_text.parse::<Decimal128>().unwrap();
    let mut docs = (0..50)
        .map(|index| doc! { "_id": index, "value": "ordinary text" })
        .collect::<Vec<_>>();
    docs.push(doc! { "_id": 50, "value": decimal });
    collection.insert_many(docs).await.unwrap();

    let connection = drivers_mongodb::MongodbDriver
        .connect(tablepro_core::ConnectOptions {
            host,
            port,
            database: "appdb".into(),
            tls: tablepro_core::TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    (container, connection, collection, decimal)
}

#[test]
fn page_metadata_does_not_replace_schema_for_other_drivers() {
    let loaded = vec![column("value", "numeric", false)];
    let page = QueryResult {
        columns: vec![column("value", "text", false)],
        rows: Vec::new(),
        truncated: false,
    };
    assert_eq!(columns_for_browse_page("postgres", &loaded, Some(&page)), loaded);
}

#[test]
fn same_count_type_change_invalidates_cached_grid_factories() {
    let rendered = vec![column("value", "string", false)];
    let current = vec![column("value", "mixed", false)];
    assert!(!column_layout_matches(&rendered, &current));
    assert!(column_layout_matches(&current, &current));
}
