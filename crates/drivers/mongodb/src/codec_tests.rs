use super::*;

use mongodb::bson::doc;

#[test]
fn missing_field_marker_cannot_be_bound_as_bson_null() {
    assert!(matches!(
        value_to_bson(&Value::Undecodable("missing BSON field".into())),
        Err(DriverError::Unsupported(_))
    ));
}

#[test]
fn mongodb_date_grid_bindings_refuse_submillisecond_rounding() {
    use chrono::{NaiveDate, TimeZone, Utc};
    use mongodb::bson::DateTime;

    let date = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
    let aligned = date.and_hms_nano_opt(12, 34, 56, 123_000_000).unwrap();
    let submillisecond = date.and_hms_nano_opt(12, 34, 56, 123_000_001).unwrap();
    assert_eq!(
        value_to_bson(&Value::DateTime(aligned)).unwrap(),
        Bson::DateTime(DateTime::from_millis(aligned.and_utc().timestamp_millis()))
    );
    assert!(matches!(
        value_to_bson(&Value::DateTime(submillisecond)),
        Err(DriverError::Unsupported(_))
    ));

    let aligned_tz = Utc.timestamp_opt(1_790_769_296, 123_000_000).unwrap();
    let submillisecond_tz = Utc.timestamp_opt(1_790_769_296, 123_000_001).unwrap();
    assert_eq!(
        value_to_bson(&Value::TimestampTz(aligned_tz)).unwrap(),
        Bson::DateTime(DateTime::from_millis(aligned_tz.timestamp_millis()))
    );
    assert!(matches!(
        value_to_bson(&Value::TimestampTz(submillisecond_tz)),
        Err(DriverError::Unsupported(_))
    ));
}

#[test]
fn mixed_scalar_bson_column_keeps_canonical_type_markers() {
    let decimal = "12345678901234567890.1234567890123"
        .parse::<mongodb::bson::Decimal128>()
        .unwrap();
    let docs = vec![
        doc! { "value": "12345678901234567890.1234567890123" },
        doc! { "value": decimal },
    ];
    let columns = columns_from_docs(&docs);
    assert_eq!(columns[0].data_type, "mixed");
    let rows = docs
        .iter()
        .map(|doc| document_to_row(doc, &columns))
        .collect::<Vec<_>>();
    assert_eq!(
        rows.iter().map(|row| row[0].clone()).collect::<Vec<_>>(),
        vec![
            Value::Json(Box::new(serde_json::json!("12345678901234567890.1234567890123"))),
            Value::Json(Box::new(
                serde_json::json!({"$numberDecimal": "12345678901234567890.1234567890123"})
            )),
        ]
    );

    let json: serde_json::Value = serde_json::from_str(&tablepro_core::export::render_json(&columns, &rows)).unwrap();
    let json_rows = json.as_array().unwrap();
    assert_eq!(json_rows[0]["value"], "12345678901234567890.1234567890123");
    assert_eq!(
        json_rows[1]["value"],
        serde_json::json!({"$numberDecimal": "12345678901234567890.1234567890123"})
    );

    let csv_text = tablepro_core::export::render_csv(&columns, &rows, &tablepro_core::export::CsvOptions::default());
    let mut csv = csv::Reader::from_reader(csv_text.as_bytes());
    let csv_values = csv
        .records()
        .map(|record| record.unwrap()[0].to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        csv_values,
        [
            serde_json::to_string("12345678901234567890.1234567890123").unwrap(),
            r#"{"$numberDecimal":"12345678901234567890.1234567890123"}"#.into(),
        ]
        .into_iter()
        .collect()
    );
}

#[test]
fn page_type_conflict_marks_column_mixed_before_decoding_late_values() {
    let decimal_text = "12345678901234567890.1234567890123";
    let sample = doc! { "value": decimal_text };
    let page = doc! { "value": decimal_text.parse::<mongodb::bson::Decimal128>().unwrap() };
    let mut columns = columns_from_docs(std::slice::from_ref(&sample));
    assert_eq!(columns[0].data_type, "string");

    merge_page_types(&mut columns, std::slice::from_ref(&page));

    assert_eq!(columns[0].data_type, "mixed");
    assert_eq!(
        document_to_row(&page, &columns),
        vec![Value::Json(Box::new(
            serde_json::json!({"$numberDecimal": decimal_text})
        ))]
    );
}

#[test]
fn arbitrary_precision_json_numbers_remain_bson_numbers() {
    let document = serde_json_to_document(
        r#"{"nested":{"n":9223372036854775807},"values":[18,1.25],"wide":18446744073709551615}"#,
    )
    .unwrap();
    assert_eq!(document.get_document("nested").unwrap().get_i64("n").unwrap(), i64::MAX);
    assert_eq!(
        document.get_array("values").unwrap(),
        &vec![Bson::Int64(18), Bson::Double(1.25)]
    );
    assert!(matches!(document.get("wide"), Some(Bson::Decimal128(_))));
    assert!(serde_json_to_document("[]").is_err());
}

#[test]
fn canonical_extended_json_import_restores_nested_bson_types() {
    use mongodb::bson::{Binary, DateTime, Decimal128, doc, oid::ObjectId, spec::BinarySubtype};

    let id = ObjectId::new();
    let input = serde_json::json!({
        "_id": {"$oid": id.to_hex()},
        "nested": {
            "amount": {"$numberDecimal": "1234567890123456789.123456789012345"},
            "when": {"$date": {"$numberLong": "1234567890123"}},
            "binary": {"$binary": {"base64": "AP9B", "subType": "80"}},
            "sequence": [{"$numberLong": "7"}],
        },
    });

    let document = serde_json_to_document(&input.to_string()).unwrap();
    let expected_decimal: Decimal128 = "1234567890123456789.123456789012345".parse().unwrap();
    assert_eq!(
        document,
        doc! {
            "_id": id,
            "nested": {
                "amount": expected_decimal,
                "when": DateTime::from_millis(1_234_567_890_123),
                "binary": Binary { subtype: BinarySubtype::UserDefined(0x80), bytes: vec![0, 255, 65] },
                "sequence": [7_i64],
            },
        }
    );
}

#[test]
fn nested_bson_special_values_keep_their_extended_json_types() {
    use mongodb::bson::{Binary, DateTime, spec::BinarySubtype};

    let document = doc! {
        "amount": Bson::Decimal128("1234567890123456789.123456789012345".parse().unwrap()),
        "blob": Bson::Binary(Binary { subtype: BinarySubtype::Generic, bytes: vec![0, 255, 65] }),
        "when": Bson::DateTime(DateTime::from_millis(1_234_567_890_123)),
    };

    assert_eq!(
        bson_to_value(&Bson::Document(document)),
        Value::Json(Box::new(serde_json::json!({
            "amount": {"$numberDecimal": "1234567890123456789.123456789012345"},
            "blob": {"$binary": {"base64": "AP9B", "subType": "00"}},
            "when": {"$date": {"$numberLong": "1234567890123"}},
        })))
    );
    assert_eq!(
        bson_to_value(&Bson::Array(vec![Bson::Decimal128(
            "1234567890123456789.123456789012345".parse().unwrap()
        )])),
        Value::Json(Box::new(serde_json::json!([
            {"$numberDecimal": "1234567890123456789.123456789012345"}
        ])))
    );
}

#[test]
fn bson_decimal_and_date_extremes_remain_exact_outside_core_ranges() {
    use mongodb::bson::DateTime;

    for text in ["1E-6176", "9.999999999999999999999999999999999E+6144", "-0.00"] {
        let decimal = Bson::Decimal128(text.parse().unwrap());
        assert_eq!(bson_to_value(&decimal), Value::Text(text.into()));
    }

    for millis in [i64::MIN, i64::MAX] {
        assert_eq!(
            bson_to_value(&Bson::DateTime(DateTime::from_millis(millis))),
            Value::Json(Box::new(
                serde_json::json!({"$date": {"$numberLong": millis.to_string()}})
            ))
        );
    }
}

#[test]
fn non_generic_binary_subtypes_keep_their_extended_json_metadata() {
    use mongodb::bson::{Binary, spec::BinarySubtype};

    for (subtype, expected) in [
        (BinarySubtype::Function, "01"),
        (BinarySubtype::BinaryOld, "02"),
        (BinarySubtype::UuidOld, "03"),
        (BinarySubtype::Uuid, "04"),
        (BinarySubtype::Md5, "05"),
        (BinarySubtype::Encrypted, "06"),
        (BinarySubtype::Column, "07"),
        (BinarySubtype::Sensitive, "08"),
        (BinarySubtype::Vector, "09"),
        (BinarySubtype::Reserved(0x0a), "0a"),
        (BinarySubtype::UserDefined(0x80), "80"),
    ] {
        let value = Bson::Binary(Binary {
            subtype,
            bytes: vec![0, 255, 65],
        });
        assert_eq!(
            bson_to_value(&value),
            Value::Json(Box::new(serde_json::json!({
                "$binary": {"base64": "AP9B", "subType": expected}
            }))),
            "subtype {expected}"
        );
    }
    assert_eq!(
        bson_to_value(&Bson::Binary(Binary {
            subtype: BinarySubtype::Generic,
            bytes: vec![0, 255, 65],
        })),
        Value::Bytes(vec![0, 255, 65])
    );
}

#[test]
fn uncommon_top_level_bson_kinds_keep_extended_json_type_markers() {
    use mongodb::bson::{JavaScriptCodeWithScope, Regex, Timestamp, oid::ObjectId};

    let db_pointer = Bson::try_from(serde_json::json!({
        "$dbPointer": {
            "$ref": "legacy.collection",
            "$id": {"$oid": "0123456789abcdef01234567"}
        }
    }))
    .unwrap();

    let uncommon = [
        (Bson::Timestamp(Timestamp { time: 42, increment: 7 }), "$timestamp"),
        (
            Bson::RegularExpression(Regex {
                pattern: "^tablepro".into(),
                options: "i".into(),
            }),
            "$regularExpression",
        ),
        (Bson::JavaScriptCode("return 1;".into()), "$code"),
        (
            Bson::JavaScriptCodeWithScope(JavaScriptCodeWithScope {
                code: "return value;".into(),
                scope: doc! { "value": 7 },
            }),
            "$scope",
        ),
        (Bson::Symbol("legacy-symbol".into()), "$symbol"),
        (Bson::ObjectId(ObjectId::new()), "$oid"),
        (db_pointer, "$dbPointer"),
        (Bson::Undefined, "$undefined"),
        (Bson::MinKey, "$minKey"),
        (Bson::MaxKey, "$maxKey"),
    ];

    for (bson, marker) in uncommon {
        let value = bson_to_value(&bson);
        let Value::Json(json) = value else {
            panic!("{bson:?} must retain its BSON kind");
        };
        assert!(json.to_string().contains(marker), "{bson:?}: {json}");
        assert_eq!(*json, bson.clone().into_canonical_extjson(), "{marker}");
        assert_eq!(
            value_to_bson(&Value::Json(json)).unwrap(),
            bson,
            "{marker} must restore the native BSON value for grid writes"
        );
    }
}

#[test]
fn bson_type_name_object_id() {
    assert_eq!(bson_type_name(&Bson::Null), "null");
    assert_eq!(bson_type_name(&Bson::String("x".into())), "string");
}
