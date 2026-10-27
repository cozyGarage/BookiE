use super::{
    ColumnInfo, MAX_QUERY_RESULT_BYTES, MAX_QUERY_RESULT_CELLS, MAX_QUERY_ROWS, QualifiedTypeName, QueryResult,
    QueryResultBatch, QueryResultBudget, Value,
};

#[test]
fn enum_catalog_metadata_does_not_change_column_info_wire_shape() {
    let column = ColumnInfo {
        name: "status".into(),
        data_type: "status_type".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: Some(QualifiedTypeName {
            schema: "app".into(),
            name: "status_type".into(),
        }),
        domain_type: None,
    };

    let wire = serde_json::to_value(&column).unwrap();
    assert!(wire.get("enum_type").is_none());
    let decoded: ColumnInfo = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded.enum_type, None);
}

#[test]
fn result_cells_stay_within_four_machine_words() {
    assert!(std::mem::size_of::<Value>() <= 32);
}

#[test]
fn result_budget_counts_json_document_storage() {
    let scalar = QueryResult {
        columns: Vec::new(),
        rows: vec![vec![Value::Int(1)]],
        truncated: false,
    };
    let json = QueryResult {
        columns: Vec::new(),
        rows: vec![vec![Value::Json(serde_json::Value::Bool(true).into())]],
        truncated: false,
    };

    assert!(json.estimated_row_bytes() >= scalar.estimated_row_bytes() + std::mem::size_of::<serde_json::Value>());
}

#[test]
fn result_budget_rejects_raw_row_before_value_decode() {
    let mut budget = QueryResultBudget::default();
    let mut decoded = false;
    let row = budget
        .decode_and_admit_row(1, MAX_QUERY_RESULT_BYTES, || {
            decoded = true;
            Ok::<_, ()>(vec![Value::Text(String::new())])
        })
        .unwrap();

    assert!(row.is_none());
    assert!(!decoded);
}

#[test]
fn result_budget_tracks_decoded_row_bytes_after_preflight() {
    let mut budget = QueryResultBudget {
        bytes: MAX_QUERY_RESULT_BYTES - 128,
        ..QueryResultBudget::default()
    };
    let expanded = budget
        .decode_and_admit_row(1, 1, || Ok::<_, ()>(vec![Value::Text("x".repeat(256))]))
        .unwrap();
    assert!(expanded.is_none());

    let row = budget
        .decode_and_admit_row(1, 1, || Ok::<_, ()>(vec![Value::Text("x".into())]))
        .unwrap();

    assert_eq!(row, Some(vec![Value::Text("x".into())]));
}

#[test]
fn result_budget_caps_cells_and_dynamic_value_storage() {
    assert_eq!(MAX_QUERY_RESULT_BYTES, 67_108_864);
    let row = vec![Value::Text("x".into())];
    let mut byte_budget = QueryResultBudget {
        bytes: super::MAX_QUERY_RESULT_BYTES - super::row_size(&row),
        ..QueryResultBudget::default()
    };
    assert!(byte_budget.admit(&row));
    assert!(!byte_budget.admit(&row));

    let one_cell = [Value::Null];
    let mut exact_cell_budget = QueryResultBudget {
        cells: MAX_QUERY_RESULT_CELLS - 1,
        ..QueryResultBudget::default()
    };
    assert!(exact_cell_budget.admit(&one_cell));
    assert!(!exact_cell_budget.admit(&one_cell));
    let mut cell_budget = QueryResultBudget {
        cells: MAX_QUERY_RESULT_CELLS,
        ..QueryResultBudget::default()
    };
    assert!(!cell_budget.admit(&[Value::Null]));
    let mut row_budget = QueryResultBudget {
        rows: MAX_QUERY_ROWS - 1,
        ..QueryResultBudget::default()
    };
    assert!(row_budget.admit(&one_cell));
    assert!(!row_budget.admit(&one_cell));
}

#[test]
fn result_budget_counts_dynamic_value_storage() {
    let text = Value::Text(String::with_capacity(32));
    let undecodable = Value::Undecodable(String::with_capacity(32));
    let bytes = Value::Bytes(vec![0; 32]);
    let json_string = serde_json::json!("a dynamically stored JSON string");
    let json_array = serde_json::json!(["a dynamically stored JSON array"]);
    let json_object = serde_json::json!({"key": "a dynamically stored JSON object"});

    for value in [&text, &undecodable, &bytes] {
        assert!(super::value_size(value) > 1);
    }
    for value in [&json_string, &json_array, &json_object] {
        assert!(super::json_heap_size(value) > 1);
        assert!(super::value_size(&Value::Json(Box::new(value.clone()))) > 1);
    }

    let empty_row = super::row_size(&[]);
    let one_cell = super::row_size(&[Value::Null]);
    assert!(empty_row > 1);
    assert!(one_cell > empty_row);
}

#[test]
fn first_result_keeps_truncation_from_either_batch_or_result_set() {
    for (batch_truncated, result_truncated, expected) in [
        (true, false, true),
        (false, true, true),
        (true, true, true),
        (false, false, false),
    ] {
        let batch = QueryResultBatch {
            result_sets: vec![QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                truncated: result_truncated,
            }],
            truncated: batch_truncated,
        };

        assert_eq!(batch.into_first().truncated, expected);
    }
}

#[test]
fn result_budget_counts_admitted_rows_to_the_limit() {
    let mut budget = QueryResultBudget {
        rows: MAX_QUERY_ROWS - 1,
        ..QueryResultBudget::default()
    };

    assert!(budget.admit(&[]));
    assert!(!budget.admit(&[]));
}

#[test]
fn value_size_counts_owned_text_bytes_and_json_storage() {
    let text = String::with_capacity(32);
    let text_capacity = text.capacity();
    assert_eq!(super::value_size(&Value::Text(text)), text_capacity);
    let undecodable = String::from("wide");
    let undecodable_capacity = undecodable.capacity();
    assert_eq!(
        super::value_size(&Value::Undecodable(undecodable)),
        undecodable_capacity
    );

    let bytes = vec![0; 24];
    let byte_capacity = bytes.capacity();
    assert_eq!(super::value_size(&Value::Bytes(bytes)), byte_capacity);

    let json_string = String::with_capacity(48);
    let json_capacity = json_string.capacity();
    assert_eq!(
        super::value_size(&Value::Json(Box::new(serde_json::Value::String(json_string)))),
        std::mem::size_of::<serde_json::Value>() + json_capacity
    );

    let json_text = String::with_capacity(48);
    let json_text_capacity = json_text.capacity();
    assert_eq!(super::value_size(&Value::JsonText(json_text)), json_text_capacity);
}

#[test]
fn json_heap_size_counts_strings_nested_arrays_and_object_keys() {
    let string = String::with_capacity(20);
    let string_capacity = string.capacity();
    assert_eq!(
        super::json_heap_size(&serde_json::Value::String(string)),
        string_capacity
    );

    let nested = String::with_capacity(30);
    let nested_capacity = nested.capacity();
    let mut array = Vec::with_capacity(3);
    array.push(serde_json::Value::String(nested));
    let array_size = array.capacity() * std::mem::size_of::<serde_json::Value>() + nested_capacity;
    assert_eq!(super::json_heap_size(&serde_json::Value::Array(array)), array_size);

    let key = String::with_capacity(12);
    let key_capacity = key.capacity();
    let value = String::with_capacity(40);
    let value_capacity = value.capacity();
    let mut object = serde_json::Map::new();
    object.insert(key, serde_json::Value::String(value));
    let object_size = std::mem::size_of::<(String, serde_json::Value)>() + key_capacity + value_capacity;
    assert_eq!(super::json_heap_size(&serde_json::Value::Object(object)), object_size);
}

#[test]
fn result_row_estimate_counts_owned_capacity_and_row_containers() {
    let mut text = String::with_capacity(128);
    text.push('x');
    let capacity = text.capacity();
    let result = QueryResult {
        columns: Vec::new(),
        rows: vec![vec![Value::Text(text)], vec![]],
        truncated: false,
    };
    assert_eq!(
        result.estimated_row_bytes(),
        capacity + std::mem::size_of::<Value>() + 2 * std::mem::size_of::<Vec<Value>>()
    );
}

#[test]
fn row_size_counts_container_and_owned_value_storage() {
    let text = String::with_capacity(64);
    let text_capacity = text.capacity();
    let row = [Value::Text(text)];

    assert_eq!(
        super::row_size(&row),
        std::mem::size_of::<Value>() + std::mem::size_of::<Vec<Value>>() + text_capacity
    );
}

#[test]
fn retained_rows_sums_all_result_sets() {
    let batch = QueryResultBatch {
        result_sets: vec![
            QueryResult {
                columns: Vec::new(),
                rows: vec![Vec::new(), Vec::new()],
                truncated: false,
            },
            QueryResult {
                columns: Vec::new(),
                rows: vec![Vec::new()],
                truncated: false,
            },
        ],
        truncated: false,
    };

    assert_eq!(batch.retained_rows(), 3);
}
