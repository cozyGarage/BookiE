use serde_json::{Value, json};

use super::{encoded_size, enforce_response_budget};

#[test]
fn response_budget_keeps_a_prefix_across_result_sets_and_marks_each_cut() {
    let response = json!({
        "rows": [["first row"], ["second row"]],
        "additional_result_sets": [{
            "rows": [["third row"]],
            "truncated": false,
        }],
        "truncated": false,
    });
    let empty_rows = json!({
        "rows": [],
        "additional_result_sets": [{"rows": [], "truncated": false}],
        "truncated": false,
    });
    let limit = encoded_size(&empty_rows).unwrap() + encoded_size(&json!(["first row"])).unwrap();

    let bounded = enforce_response_budget(response, limit).unwrap();

    assert!(encoded_size(&bounded).unwrap() <= limit);
    assert_eq!(bounded["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        bounded["additional_result_sets"][0]["rows"].as_array().unwrap().len(),
        0
    );
    assert_eq!(bounded["additional_result_sets"][0]["truncated"], true);
    assert_eq!(bounded["truncated"], true);
}

#[test]
fn metadata_that_cannot_fit_is_returned_as_an_error() {
    let response = json!({"columns": ["very long column metadata"]});

    assert!(enforce_response_budget(response, 1).is_err());
}

#[test]
fn truncation_marker_reserves_space_before_keeping_rows() {
    let response = json!({
        "plan_rows": [["first row"], ["second row is long enough to fund the marker"], ["third row"]],
    });
    let largest_prefix_without_marker = json!({
        "plan_rows": [["first row"], ["second row is long enough to fund the marker"]],
    });
    let limit = encoded_size(&largest_prefix_without_marker).unwrap();

    let bounded = enforce_response_budget(response, limit).unwrap();

    assert_eq!(bounded["plan_rows"].as_array().unwrap().len(), 1);
    assert_eq!(bounded["truncated"], true);
    assert!(encoded_size(&bounded).unwrap() <= limit);
}

#[test]
fn row_arrays_are_counted_without_allocating_a_serialized_copy() {
    let response = json!({"rows": [["value"]]});
    let encoded = serde_json::to_vec(&response).unwrap();
    let count = encoded_size(&response).unwrap();

    assert_eq!(count, encoded.len());
    assert_eq!(response["rows"][0][0], Value::String("value".into()));
}
