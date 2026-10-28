use super::*;
use crate::export::test_support::column;

#[test]
fn value_contract_nonfinite_numbers_remain_distinct_from_sql_null() {
    let columns = vec![column("value")];
    let rows = vec![
        vec![Value::Float(f64::NAN)],
        vec![Value::Float(f64::INFINITY)],
        vec![Value::Float(f64::NEG_INFINITY)],
        vec![Value::Null],
    ];
    let output: serde_json::Value = serde_json::from_str(&render_json(&columns, &rows)).unwrap();
    assert_eq!(
        output,
        serde_json::json!([{"value": "NaN"}, {"value": "inf"}, {"value": "-inf"}, {"value": null}])
    );
}

#[test]
fn json_text_export_keeps_source_tokens_and_duplicate_keys() {
    let columns = vec![column("value")];
    let source = "{ \"z\": 1, \"z\": 2, \"n\": 1.0000000000000000000000000000000000000001 }";
    let row = vec![Value::JsonText(source.into())];

    let output = render_json(&columns, std::slice::from_ref(&row));
    let row_output = render_row_json(&columns, &row);

    assert!(output.contains(source));
    assert!(row_output.contains(source));
    assert!(!row_output.contains("\\\"z\\\""));
}

#[test]
fn value_contract_single_row_json_export_preserves_value_types() {
    let columns = vec![column("id"), column("payload"), column("enabled"), column("note")];
    let row = vec![
        Value::Int(9_007_199_254_740_993),
        Value::Json(Box::new(
            serde_json::json!({"$numberDecimal": "1234567890123456789.123456789012345"}),
        )),
        Value::Bool(true),
        Value::Null,
    ];

    assert_eq!(
        row_to_json(&columns, &row),
        serde_json::json!({
            "id": 9_007_199_254_740_993_i64,
            "payload": {"$numberDecimal": "1234567890123456789.123456789012345"},
            "enabled": true,
            "note": null,
        })
    );
}

#[test]
fn value_contract_clickhouse_wide_integers_remain_exact_json_strings() {
    let columns = vec![column("signed_min"), column("signed_max"), column("unsigned_max")];
    let rows = vec![vec![
        Value::Text("-170141183460469231731687303715884105728".into()),
        Value::Text("170141183460469231731687303715884105727".into()),
        Value::Text("340282366920938463463374607431768211455".into()),
    ]];

    let output: serde_json::Value = serde_json::from_str(&render_json(&columns, &rows)).unwrap();
    assert_eq!(
        output,
        serde_json::json!([{
            "signed_min": "-170141183460469231731687303715884105728",
            "signed_max": "170141183460469231731687303715884105727",
            "unsigned_max": "340282366920938463463374607431768211455",
        }])
    );
}

#[test]
fn value_contract_json_keeps_null_and_booleans_distinct_from_text() {
    let columns = vec![column("value")];
    let rows = vec![
        vec![Value::Bool(true)],
        vec![Value::Text("true".into())],
        vec![Value::Bool(false)],
        vec![Value::Text("false".into())],
        vec![Value::Null],
        vec![Value::Text("null".into())],
    ];
    let output: serde_json::Value = serde_json::from_str(&render_json(&columns, &rows)).unwrap();

    assert_eq!(
        output,
        serde_json::json!([
            {"value": true},
            {"value": "true"},
            {"value": false},
            {"value": "false"},
            {"value": null},
            {"value": "null"}
        ])
    );
}

#[test]
fn value_contract_json_keeps_negative_zero_distinct_from_positive_zero_and_null() {
    let columns = vec![column("value")];
    let rows = vec![vec![Value::Float(-0.0)], vec![Value::Float(0.0)], vec![Value::Null]];
    let text = render_json(&columns, &rows);
    assert!(text.contains("\"value\": -0.0"), "{text}");
    let output: serde_json::Value = serde_json::from_str(&text).unwrap();
    let values = output.as_array().unwrap();
    assert_eq!(values[0]["value"].as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
    assert_eq!(values[1]["value"].as_f64().unwrap().to_bits(), 0.0f64.to_bits());
    assert!(values[2]["value"].is_null());
}

#[test]
fn value_contract_json_round_trip_preserves_each_finite_exponent_and_mantissa_edges() {
    let columns = vec![column("value")];
    let mantissas = [0, 1, 1 << 51, (1 << 52) - 2, (1 << 52) - 1];
    let mut expected = Vec::with_capacity(2 * 2048 * mantissas.len());
    for sign in [0, 1_u64 << 63] {
        for exponent in 0..0x7ff_u64 {
            for mantissa in mantissas {
                expected.push(f64::from_bits(sign | (exponent << 52) | mantissa));
            }
        }
    }
    let rows = expected
        .iter()
        .map(|value| vec![Value::Float(*value)])
        .collect::<Vec<_>>();

    let text = render_json(&columns, &rows);
    let output: serde_json::Value = serde_json::from_str(&text).unwrap();
    let values = output.as_array().expect("JSON export must be an array");
    assert_eq!(values.len(), expected.len());
    for (index, (json_row, expected)) in values.iter().zip(expected).enumerate() {
        let actual = json_row["value"].as_f64().expect("finite float must be a JSON number");
        assert_eq!(actual.to_bits(), expected.to_bits(), "f64 case {index}");
    }
}

#[test]
fn json_renderer_keeps_value_types_duplicate_columns_and_missing_cells() {
    let columns = vec![column("id"), column("id"), column("id_2"), column("payload")];
    let rows = vec![
        vec![
            Value::Int(7),
            Value::Float(1.5),
            Value::Decimal("12.30".parse().unwrap()),
            Value::Json(Box::new(serde_json::json!({"ok": true}))),
        ],
        vec![Value::Int(8)],
    ];

    assert_eq!(json_field_names(&columns), vec!["id", "id_3", "id_2", "payload"]);
    assert_eq!(
        render_json(&columns, &rows),
        "[\n  {\n    \"id\": 7,\n    \"id_3\": 1.5,\n    \"id_2\": 12.30,\n    \"payload\": {\n      \"ok\": true\n    }\n  },\n  {\n    \"id\": 8,\n    \"id_3\": null,\n    \"id_2\": null,\n    \"payload\": null\n  }\n]"
    );
}

#[test]
fn json_field_names_make_progress_when_duplicate_names_have_gaps() {
    const CHILD: &str = "BOOKIEE_JSON_FIELD_NAMES_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let columns = vec![column("id"), column("id"), column("id_2")];
        assert_eq!(json_field_names(&columns), ["id", "id_3", "id_2"]);
        return;
    }

    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "export::json::tests::json_field_names_make_progress_when_duplicate_names_have_gaps",
        ])
        .env(CHILD, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "JSON field-name child failed: {status}");
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("JSON field-name generation did not finish within two seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
