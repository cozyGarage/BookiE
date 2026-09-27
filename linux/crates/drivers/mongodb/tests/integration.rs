#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use drivers_mongodb::MongodbDriver;
use tablepro_core::{ConnectOptions, DatabaseDriver, TlsConfig, Value};
use testcontainers::{ContainerAsync, ImageExt};
use testcontainers_modules::mongo::Mongo;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// The module's own default tag is MongoDB 5.0.6, which is long out of
/// support and is not the server this driver is verified against. Pin the
/// same major the TLS fixture uses so both tiers exercise one version.
const MONGO_TAG: &str = "7";

async fn start_mongo() -> (ContainerAsync<Mongo>, String, u16) {
    let container = Mongo::default()
        .with_tag(MONGO_TAG)
        .start()
        .await
        .expect("start mongo container");
    let host = container.get_host().await.expect("host").to_string();
    let port = container.get_host_port_ipv4(27017).await.expect("port");
    (container, host, port)
}

fn opts(host: &str, port: u16, database: &str) -> ConnectOptions {
    ConnectOptions {
        host: host.to_string(),
        port,
        database: database.to_string(),
        username: String::new(),
        password: secrecy::SecretString::new(String::new().into()),
        tls: TlsConfig::disabled(),
        ..Default::default()
    }
}

async fn seeded_connection(host: &str, port: u16) -> Box<dyn tablepro_core::Connection> {
    let conn = MongodbDriver.connect(opts(host, port, "appdb")).await.expect("connect");
    conn.execute(r#"db.people.insertOne({"name": "ada", "team": "core"})"#)
        .await
        .expect("seed ada");
    conn.execute(r#"db.people.insertOne({"name": "grace", "team": "core"})"#)
        .await
        .expect("seed grace");
    conn.execute(r#"db.people.insertOne({"name": "alan", "team": "ops"})"#)
        .await
        .expect("seed alan");
    conn
}

fn column_values(result: &tablepro_core::QueryResult, column: &str) -> Vec<String> {
    let index = result
        .columns
        .iter()
        .position(|c| c.name == column)
        .unwrap_or_else(|| panic!("column {column} missing from {:?}", result.columns));
    result
        .rows
        .iter()
        .filter_map(|row| match row.get(index) {
            Some(Value::Text(text)) => Some(text.clone()),
            Some(Value::Json(json)) => Some(json.to_string().trim_matches('"').to_string()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_inserted_document_is_listed_browsed_and_found() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let collections = conn.list_tables().await.expect("list collections");
    assert!(
        collections.iter().any(|c| c.name == "people"),
        "the seeded collection must be listed: {collections:?}"
    );

    let browsed = conn.fetch_rows(None, "people", 0, 10).await.expect("browse people");
    assert_eq!(browsed.rows.len(), 3, "browse must return every seeded document");

    let found = conn
        .query(r#"db.people.find({"team": "core"})"#)
        .await
        .expect("find by filter");
    let mut names = column_values(&found, "name");
    names.sort();
    assert_eq!(names, vec!["ada".to_string(), "grace".to_string()]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_filtered_delete_removes_only_the_matching_documents() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let deleted = conn
        .execute(r#"db.people.deleteMany({"team": "ops"})"#)
        .await
        .expect("delete ops");
    assert_eq!(deleted.rows_affected, 1, "only the ops document matches");

    let remaining = conn.fetch_rows(None, "people", 0, 10).await.expect("browse people");
    assert_eq!(remaining.rows.len(), 2, "the core documents must survive");
    let mut names = column_values(&remaining, "name");
    names.sort();
    assert_eq!(names, vec!["ada".to_string(), "grace".to_string()]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_aggregate_pipeline_groups_documents() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let result = conn
        .query(r#"db.people.aggregate([{"$group": {"_id": "$team", "total": {"$sum": 1}}}])"#)
        .await
        .expect("aggregate by team");

    assert_eq!(result.rows.len(), 2, "core and ops must each produce a group");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn a_dropped_collection_stops_being_listed() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    conn.execute("DROP TABLE people").await.expect("drop people");

    let collections = conn.list_tables().await.expect("list collections");
    assert!(
        !collections.iter().any(|c| c.name == "people"),
        "the dropped collection must disappear: {collections:?}"
    );
}

#[tokio::test]
#[ignore = "requires docker"]
async fn an_unsupported_statement_is_refused_rather_than_silently_ignored() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let error = conn
        .execute("UPDATE people SET name = 'x'")
        .await
        .expect_err("the driver documents insertOne/deleteMany/DROP TABLE only");
    let message = format!("{error}");
    assert!(
        message.contains("insertOne") || message.contains("unsupported"),
        "the refusal must say what is supported, got: {message}"
    );

    let survivors = conn.fetch_rows(None, "people", 0, 10).await.expect("browse people");
    assert_eq!(survivors.rows.len(), 3, "a refused statement must change nothing");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn browsing_a_missing_collection_is_empty_rather_than_an_error() {
    let (_container, host, port) = start_mongo().await;
    let conn = seeded_connection(&host, port).await;

    let browsed = conn
        .fetch_rows(None, "absent", 0, 10)
        .await
        .expect("browsing an absent collection must not fail");
    assert!(browsed.rows.is_empty(), "an absent collection has no rows");
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_preserves_numbers_and_text_through_json_commands() {
    let (_container, host, port) = start_mongo().await;
    let connection = MongodbDriver.connect(opts(&host, port, "appdb")).await.unwrap();
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../../testdata/value-contract.json")).unwrap();
    let mut cases = vec![(serde_json::Value::Null, Value::Null)];
    for text in corpus["integers"].as_array().unwrap() {
        let number: i64 = text.as_str().unwrap().parse().unwrap();
        cases.push((serde_json::json!(number), Value::Int(number)));
    }
    for text in corpus["floats"].as_array().unwrap() {
        let number: f64 = text.as_str().unwrap().parse().unwrap();
        cases.push((serde_json::json!(number), Value::Float(number)));
    }
    for text in corpus["texts"].as_array().unwrap() {
        cases.push((text.clone(), Value::Text(text.as_str().unwrap().into())));
    }
    let long = "x".repeat(corpus["long_text_bytes"].as_u64().unwrap() as usize);
    cases.push((serde_json::json!(&long), Value::Text(long)));
    for (id, (input, expected)) in cases.into_iter().enumerate() {
        let document = serde_json::json!({"_id": id, "value": input});
        connection
            .execute(&format!("db.values.insertOne({document})"))
            .await
            .unwrap();
        let query = format!("db.values.find({{\"_id\":{id}}})");
        let result = connection.query(&query).await.unwrap();
        let column = result.columns.iter().position(|column| column.name == "value").unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][column], expected);
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn nested_bson_special_values_keep_exact_extended_json_types() {
    use mongodb::bson::{Binary, DateTime, Decimal128, doc, spec::BinarySubtype};

    let (_container, host, port) = start_mongo().await;
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}/appdb"))
        .await
        .expect("connect direct BSON fixture client");
    let document = doc! {
        "_id": "special",
        "decimal_min": "1E-6176".parse::<Decimal128>().unwrap(),
        "decimal_max": "9.999999999999999999999999999999999E+6144"
            .parse::<Decimal128>()
            .unwrap(),
        "date_min": DateTime::from_millis(i64::MIN),
        "date_max": DateTime::from_millis(i64::MAX),
        "uuid_binary": Binary { subtype: BinarySubtype::Uuid, bytes: (0..16).collect() },
        "user_binary": Binary { subtype: BinarySubtype::UserDefined(0x80), bytes: vec![0, 255, 65] },
        "nested": {
            "amount": "1234567890123456789.123456789012345"
                .parse::<Decimal128>()
                .unwrap(),
            "blob": Binary { subtype: BinarySubtype::Generic, bytes: vec![0, 255, 65] },
            "when": DateTime::from_millis(1_234_567_890_123),
        },
    };
    client
        .database("appdb")
        .collection::<mongodb::bson::Document>("special_values")
        .insert_one(document)
        .await
        .expect("insert exact BSON fixture");

    let connection = MongodbDriver.connect(opts(&host, port, "appdb")).await.unwrap();
    let result = connection
        .query(r#"db.special_values.find({"_id":"special"})"#)
        .await
        .expect("read exact BSON fixture");
    let nested = result
        .columns
        .iter()
        .position(|column| column.name == "nested")
        .unwrap();
    let column = |name: &str| result.columns.iter().position(|column| column.name == name).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][column("decimal_min")], Value::Text("1E-6176".into()));
    assert_eq!(
        result.rows[0][column("decimal_max")],
        Value::Text("9.999999999999999999999999999999999E+6144".into())
    );
    for millis in [i64::MIN, i64::MAX] {
        let name = if millis < 0 { "date_min" } else { "date_max" };
        assert_eq!(
            result.rows[0][column(name)],
            Value::Json(serde_json::json!({"$date": {"$numberLong": millis.to_string()}}))
        );
    }
    for (name, expected) in [
        (
            "uuid_binary",
            serde_json::json!({"$binary": {"base64": "AAECAwQFBgcICQoLDA0ODw==", "subType": "04"}}),
        ),
        (
            "user_binary",
            serde_json::json!({"$binary": {"base64": "AP9B", "subType": "80"}}),
        ),
    ] {
        assert_eq!(result.rows[0][column(name)], Value::Json(expected));
    }
    assert_eq!(
        result.rows[0][nested],
        Value::Json(serde_json::json!({
            "amount": {"$numberDecimal": "1234567890123456789.123456789012345"},
            "blob": {"$binary": {"base64": "AP9B", "subType": "00"}},
            "when": {"$date": {"$numberLong": "1234567890123"}},
        }))
    );

    let exported: serde_json::Value =
        serde_json::from_str(&tablepro_core::export::render_json(&result.columns, &result.rows))
            .expect("parse JSON export");
    assert_eq!(
        exported[0]["nested"],
        serde_json::json!({
            "amount": {"$numberDecimal": "1234567890123456789.123456789012345"},
            "blob": {"$binary": {"base64": "AP9B", "subType": "00"}},
            "when": {"$date": {"$numberLong": "1234567890123"}},
        }),
        "JSON export must keep BSON extended types nested"
    );
    assert_eq!(
        exported[0]["uuid_binary"],
        serde_json::json!({"$binary": {"base64": "AAECAwQFBgcICQoLDA0ODw==", "subType": "04"}}),
        "JSON export must keep the UUID binary subtype"
    );

    let csv_output = tablepro_core::export::render_csv(
        &result.columns,
        &result.rows,
        &tablepro_core::export::CsvOptions::default(),
    );
    let mut csv_reader = csv::Reader::from_reader(csv_output.as_bytes());
    let headers = csv_reader.headers().expect("CSV headers").clone();
    let record = csv_reader.records().next().expect("CSV row").expect("valid CSV row");
    let nested_index = headers.iter().position(|header| header == "nested").unwrap();
    let nested_csv: serde_json::Value = serde_json::from_str(&record[nested_index]).expect("nested JSON in CSV");
    assert_eq!(
        nested_csv, exported[0]["nested"],
        "CSV quoting must preserve nested BSON JSON"
    );

    let directory = tempfile::tempdir().expect("temporary export directory");
    let workbook_path = directory.path().join("mongo-values.xlsx");
    tablepro_core::export::write_result_file(
        &workbook_path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .expect("export MongoDB result to XLSX");
    let nested_cell = xlsx_shared_cell_text(&workbook_path, nested, 2).expect("read nested XLSX cell");
    if let Some(marker) = missing_nested_xlsx_marker(&nested_cell) {
        panic!("nested XLSX cell missing {marker}: {nested_cell}");
    }
}

const NESTED_XLSX_DECIMAL: &str = r#""$numberDecimal":"1234567890123456789.123456789012345""#;
const NESTED_XLSX_DATE: &str = r#""$date":{"$numberLong":"1234567890123"}"#;
const NESTED_XLSX_BINARY: &str = r#""subType":"00""#;

fn missing_nested_xlsx_marker(cell: &str) -> Option<&'static str> {
    [NESTED_XLSX_DECIMAL, NESTED_XLSX_DATE, NESTED_XLSX_BINARY]
        .into_iter()
        .find(|marker| !cell.contains(marker))
}

fn excel_column_name(index: usize) -> String {
    let mut letters = Vec::new();
    let mut remainder = index;
    loop {
        let offset = u8::try_from(remainder % 26).unwrap();
        letters.push(char::from(b'A' + offset));
        if remainder < 26 {
            break;
        }
        remainder = remainder / 26 - 1;
    }
    letters.iter().rev().collect()
}

fn unescape_xml(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        decoded.push_str(&rest[..amp]);
        let Some(end) = rest[amp..].find(';') else {
            decoded.push_str(&rest[amp..]);
            return decoded;
        };
        let entity = &rest[amp..=amp + end];
        decoded.push_str(match entity {
            "&amp;" => "&",
            "&lt;" => "<",
            "&gt;" => ">",
            "&quot;" => "\"",
            "&apos;" => "'",
            _ => entity,
        });
        rest = &rest[amp + end + 1..];
    }
    decoded.push_str(rest);
    decoded
}

fn xml_text_content(entry: &str) -> String {
    let mut text = String::new();
    let mut rest = entry;
    while let Some(start) = rest.find("<t") {
        let from_tag = &rest[start..];
        let Some(gt) = from_tag.find('>') else {
            break;
        };
        if from_tag[..gt].ends_with('/') {
            rest = &from_tag[gt + 1..];
            continue;
        }
        let after = &from_tag[gt + 1..];
        let Some(close) = after.find("</t>") else {
            break;
        };
        text.push_str(&unescape_xml(&after[..close]));
        rest = &after[close + 4..];
    }
    text
}

fn worksheet_cell<'a>(sheet: &'a str, reference: &str) -> Option<&'a str> {
    let token = format!("r=\"{reference}\"");
    let at = sheet.find(&token)?;
    let start = sheet[..at].rfind("<c ")?;
    let end = sheet[at..].find("</c>")? + at + "</c>".len();
    Some(&sheet[start..end])
}

fn shared_string_index(element: &str) -> Option<usize> {
    let start = element.find("<v>")? + "<v>".len();
    let end = start + element[start..].find("</v>")?;
    element[start..end].trim().parse().ok()
}

fn shared_string_at(strings: &str, index: usize) -> Option<String> {
    let entry = strings.split("<si>").nth(index + 1)?;
    let end = entry.find("</si>").unwrap_or(entry.len());
    Some(xml_text_content(&entry[..end]))
}

fn read_zip_text(path: &std::path::Path, name: &str) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    let mut entry = archive.by_name(name).map_err(|error| format!("{name}: {error}"))?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text).map_err(|error| error.to_string())?;
    Ok(text)
}

fn xlsx_shared_cell_text(path: &std::path::Path, column: usize, row: u32) -> Result<String, String> {
    let sheet = read_zip_text(path, "xl/worksheets/sheet1.xml")?;
    let strings = read_zip_text(path, "xl/sharedStrings.xml")?;
    let reference = format!("{}{row}", excel_column_name(column));
    let element = worksheet_cell(&sheet, &reference).ok_or_else(|| format!("missing cell {reference}"))?;
    if !element.contains("t=\"s\"") {
        return Err(format!("cell {reference} is not a shared string: {element}"));
    }
    let index =
        shared_string_index(element).ok_or_else(|| format!("cell {reference} has no string index: {element}"))?;
    shared_string_at(&strings, index).ok_or_else(|| format!("shared string {index} missing"))
}

fn json_column(name: &str) -> tablepro_core::ColumnInfo {
    tablepro_core::ColumnInfo {
        name: name.into(),
        data_type: "json".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
    }
}

fn nested_extended_json(include_date: bool, include_binary: bool) -> serde_json::Value {
    let mut nested = serde_json::Map::new();
    nested.insert(
        "amount".into(),
        serde_json::json!({"$numberDecimal": "1234567890123456789.123456789012345"}),
    );
    if include_binary {
        nested.insert(
            "blob".into(),
            serde_json::json!({"$binary": {"base64": "AP9B", "subType": "00"}}),
        );
    }
    if include_date {
        nested.insert(
            "when".into(),
            serde_json::json!({"$date": {"$numberLong": "1234567890123"}}),
        );
    }
    serde_json::Value::Object(nested)
}

fn write_nested_xlsx(nested: serde_json::Value) -> (tempfile::TempDir, std::path::PathBuf) {
    let result = tablepro_core::QueryResult {
        columns: ["date_max", "uuid_binary", "user_binary", "nested"]
            .into_iter()
            .map(json_column)
            .collect(),
        rows: vec![vec![
            Value::Json(serde_json::json!({"$date": {"$numberLong": i64::MAX.to_string()}})),
            Value::Json(serde_json::json!({"$binary": {"base64": "AAECAwQFBgcICQoLDA0ODw==", "subType": "04"}})),
            Value::Json(serde_json::json!({"$binary": {"base64": "AP9B", "subType": "80"}})),
            Value::Json(nested),
        ]],
        truncated: false,
    };
    let directory = tempfile::tempdir().expect("temporary export directory");
    let path = directory.path().join("mongo-values.xlsx");
    tablepro_core::export::write_result_file(
        &path,
        &result,
        &tablepro_core::export::ResultExport {
            format: tablepro_core::export::ResultFormat::Xlsx,
            csv: &tablepro_core::export::CsvOptions::default(),
            sql: None,
        },
        || false,
        |_| {},
    )
    .expect("export nested fixture to XLSX");
    (directory, path)
}

#[test]
fn xlsx_nested_cell_requires_its_own_date_and_binary_markers() {
    let (_full_dir, full_path) = write_nested_xlsx(nested_extended_json(true, true));
    let nested_cell = xlsx_shared_cell_text(&full_path, 3, 2).expect("nested cell");
    assert_eq!(missing_nested_xlsx_marker(&nested_cell), None, "{nested_cell}");
    assert!(!nested_cell.contains(r#""subType":"04""#), "{nested_cell}");
    assert!(!nested_cell.contains(r#""subType":"80""#), "{nested_cell}");
    let top_level_date = xlsx_shared_cell_text(&full_path, 0, 2).expect("top-level date cell");
    assert!(top_level_date.contains("$numberLong"), "{top_level_date}");
    assert!(!top_level_date.contains("1234567890123"), "{top_level_date}");

    let (_dropped_date_dir, dropped_date_path) = write_nested_xlsx(nested_extended_json(false, true));
    let dropped_date_cell = xlsx_shared_cell_text(&dropped_date_path, 3, 2).expect("nested cell without date");
    assert_eq!(missing_nested_xlsx_marker(&dropped_date_cell), Some(NESTED_XLSX_DATE));
    assert!(dropped_date_cell.contains(NESTED_XLSX_DECIMAL), "{dropped_date_cell}");
    assert!(dropped_date_cell.contains(NESTED_XLSX_BINARY), "{dropped_date_cell}");
    let dropped_date_workbook = read_zip_text(&dropped_date_path, "xl/sharedStrings.xml").expect("shared strings");
    assert!(dropped_date_workbook.contains("$numberLong"), "{dropped_date_workbook}");
    assert!(
        dropped_date_workbook.contains(r#""subType":"04""#),
        "{dropped_date_workbook}"
    );
    assert!(
        dropped_date_workbook.contains(r#""subType":"80""#),
        "{dropped_date_workbook}"
    );

    let (_dropped_binary_dir, dropped_binary_path) = write_nested_xlsx(nested_extended_json(true, false));
    let dropped_binary_cell = xlsx_shared_cell_text(&dropped_binary_path, 3, 2).expect("nested cell without binary");
    assert_eq!(
        missing_nested_xlsx_marker(&dropped_binary_cell),
        Some(NESTED_XLSX_BINARY)
    );
    assert!(dropped_binary_cell.contains(NESTED_XLSX_DATE), "{dropped_binary_cell}");
    let dropped_binary_workbook = read_zip_text(&dropped_binary_path, "xl/sharedStrings.xml").expect("shared strings");
    assert!(
        dropped_binary_workbook.contains(r#""subType":"04""#),
        "{dropped_binary_workbook}"
    );
    assert!(
        dropped_binary_workbook.contains(r#""subType":"80""#),
        "{dropped_binary_workbook}"
    );
    assert!(
        dropped_binary_workbook.contains("$numberLong"),
        "{dropped_binary_workbook}"
    );
}
