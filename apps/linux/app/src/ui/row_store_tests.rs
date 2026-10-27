use super::*;
use std::cell::Cell;
use std::rc::Rc;
use tablepro_core::Value;

fn rows(count: i64) -> Vec<Vec<Value>> {
    (0..count).map(|n| vec![Value::Int(n)]).collect()
}

fn first_cell(store: &RowStore, position: u32) -> Value {
    let row = store.item(position).and_downcast::<RowObject>();
    row.map(|row| row.cell_value(0)).unwrap_or(Value::Null)
}

#[test]
fn no_row_object_exists_until_a_row_is_requested() {
    let store = RowStore::from_shared(shared(1000));
    assert_eq!(store.n_items(), 1000);
    assert_eq!(store.materialized(), 0);
    let row = store.item(41).and_downcast::<RowObject>().unwrap();
    assert_eq!(row.cell_value(0), Value::Int(41));
    assert_eq!(store.materialized(), 1);
    drop(row);
    assert_eq!(store.materialized(), 0);
    assert_eq!(first_cell(&store, 41), Value::Int(41));
    assert_eq!(store.materialized(), 0);
}

#[test]
fn a_requested_row_keeps_its_identity_and_its_edits() {
    let store = RowStore::from_shared(shared(3));
    let first = store.item(1).and_downcast::<RowObject>().unwrap();
    first.set_cell(0, Value::Int(99));
    let again = store.item(1).and_downcast::<RowObject>().unwrap();
    assert_eq!(first, again);
    assert_eq!(again.cell_value(0), Value::Int(99));
    assert_eq!(store.find(&again), Some(1));
}

#[test]
fn source_identity_tracks_rows_after_local_reordering() {
    let store = RowStore::from_shared(shared(3));

    assert!(store.reorder_source_rows(&[2, 0, 1]));
    assert_eq!(
        (0..3)
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(2), Some(0), Some(1)]
    );
    assert_eq!(first_cell(&store, 0), Value::Int(2));
}

#[test]
fn a_local_filter_can_show_a_unique_subset_of_source_rows() {
    let store = RowStore::from_shared(shared(3));

    assert!(store.replace_source_rows(&[2, 0]));

    assert_eq!(store.n_items(), 2);
    assert_eq!(store.source_index_at(0), Some(2));
    assert_eq!(store.source_index_at(1), Some(0));
    assert_eq!(first_cell(&store, 0), Value::Int(2));
}

#[test]
fn a_local_filter_rejects_duplicate_and_out_of_range_source_rows() {
    let store = RowStore::from_shared(shared(3));

    assert!(!store.replace_source_rows(&[1, 1]));
    assert!(!store.replace_source_rows(&[3]));
    assert_eq!(store.n_items(), 3);
}

#[test]
fn local_reordering_rejects_non_permutations_and_draft_rows() {
    let store = RowStore::from_shared(shared(3));
    assert!(!store.reorder_source_rows(&[0, 0, 2]));
    assert!(!store.reorder_source_rows(&[0, 1]));

    store.insert(0, &RowObject::new_draft(7, vec![Value::Int(-1)]));
    assert!(!store.reorder_source_rows(&[0, 1, 2, 3]));
}

#[test]
fn a_released_clean_row_can_be_recreated_from_the_shared_result() {
    let store = RowStore::from_shared(shared(3));
    let first = store.item(1).and_downcast::<RowObject>().unwrap();
    let held_identity = first.clone();
    assert_eq!(store.materialized(), 1);
    drop(first);

    let again = store.item(1).and_downcast::<RowObject>().unwrap();
    assert_eq!(again, held_identity);
    assert_eq!(again.cell_value(0), Value::Int(1));
    assert_eq!(store.materialized(), 1);
    drop(held_identity);
    assert_eq!(store.materialized(), 1);
    drop(again);
    assert_eq!(store.materialized(), 0);

    let recreated = store.item(1).and_downcast::<RowObject>().unwrap();
    assert_eq!(recreated.cell_value(0), Value::Int(1));
    assert_eq!(store.materialized(), 1);
}

#[test]
fn draft_rows_remain_owned_by_the_store() {
    let store = RowStore::from_shared(shared(1));
    let draft = RowObject::new_draft(7, vec![Value::Int(-1)]);
    store.insert(0, &draft);
    drop(draft);

    assert_eq!(store.materialized(), 1);
    let again = store.item(0).and_downcast::<RowObject>().unwrap();
    assert_eq!(again.draft_id(), Some(7));
    assert_eq!(again.cell_value(0), Value::Int(-1));
}

#[test]
fn an_out_of_range_position_yields_nothing() {
    let store = RowStore::from_shared(shared(2));
    assert!(store.item(2).is_none());
}

#[test]
fn insert_remove_and_splice_move_rows_and_report_the_change() {
    let store = RowStore::from_shared(shared(3));
    let changes = Rc::new(Cell::new((0, 0, 0)));
    let seen = changes.clone();
    store.connect_items_changed(move |_, position, removed, added| seen.set((position, removed, added)));

    store.insert(0, &RowObject::new_draft(7, vec![Value::Int(-1)]));
    assert_eq!(changes.get(), (0, 0, 1));
    assert_eq!(store.n_items(), 4);
    assert_eq!(first_cell(&store, 0), Value::Int(-1));
    assert_eq!(first_cell(&store, 1), Value::Int(0));

    store.remove(0);
    assert_eq!(changes.get(), (0, 1, 0));
    assert_eq!(first_cell(&store, 0), Value::Int(0));

    store.splice(1, 1, &[RowObject::new(vec![Value::Int(50)])]);
    assert_eq!(changes.get(), (1, 1, 1));
    assert_eq!(first_cell(&store, 1), Value::Int(50));
    assert_eq!(first_cell(&store, 2), Value::Int(2));
}

#[test]
fn replacing_the_rows_reports_one_change_and_drops_old_objects() {
    let old = shared(4);
    let store = RowStore::from_shared(old.clone());
    let held = store.item(0).and_downcast::<RowObject>().unwrap();
    let changes = Rc::new(Cell::new((0, 0, 0)));
    let seen = changes.clone();
    store.connect_items_changed(move |_, position, removed, added| seen.set((position, removed, added)));

    store.replace_shared(shared(2));
    assert_eq!(changes.get(), (0, 4, 2));
    assert_eq!(store.n_items(), 2);
    assert_eq!(store.materialized(), 0);
    assert_eq!(store.find(&held), None);
    assert_eq!(held.cell_value(0), Value::Int(0));
    assert_eq!(Arc::strong_count(&old), 1);
}

#[test]
fn replacing_shared_rows_preserves_edits_on_held_rows() {
    let old = shared(2);
    let store = RowStore::from_shared(old.clone());
    let held = store.item(0).and_downcast::<RowObject>().unwrap();
    held.set_cell(0, Value::Int(99));

    store.replace_shared(shared(1));

    assert_eq!(held.cell_value(0), Value::Int(99));
    assert_eq!(Arc::strong_count(&old), 1);
}

#[test]
fn a_held_row_survives_store_disposal_without_retaining_the_result() {
    let result = shared(2);
    let store = RowStore::from_shared(result.clone());
    let held = store.item(1).and_downcast::<RowObject>().unwrap();

    drop(store);

    assert_eq!(held.cell_value(0), Value::Int(1));
    assert_eq!(Arc::strong_count(&result), 1);
}

#[test]
fn a_handler_may_read_the_model_while_it_is_changing() {
    let store = RowStore::from_shared(shared(2));
    let reader = store.clone();
    store.connect_items_changed(move |_, _, _, _| {
        let _ = reader.item(0);
    });
    store.replace_shared(shared(3));
    store.remove(0);
    assert_eq!(store.n_items(), 2);
}

fn shared(count: i64) -> Arc<QueryResult> {
    Arc::new(QueryResult {
        columns: Vec::new(),
        rows: rows(count),
        truncated: false,
    })
}

#[test]
fn a_shared_result_is_read_in_place_and_never_copied_up_front() {
    let result = shared(500);
    let store = RowStore::from_shared(result.clone());
    assert_eq!(store.n_items(), 500);
    assert_eq!(store.materialized(), 0);
    assert_eq!(Arc::strong_count(&result), 2);
    let row = store.item(499).and_downcast::<RowObject>().unwrap();
    assert_eq!(row.cell_value(0), Value::Int(499));
    assert_eq!(store.materialized(), 1);
}

#[test]
fn shared_rows_survive_a_draft_inserted_above_them() {
    let store = RowStore::from_shared(shared(3));
    store.insert(0, &RowObject::new_draft(1, vec![Value::Int(-1)]));
    assert_eq!(first_cell(&store, 0), Value::Int(-1));
    assert_eq!(first_cell(&store, 3), Value::Int(2));
    store.remove(0);
    assert_eq!(first_cell(&store, 0), Value::Int(0));
}

#[test]
fn replacing_a_shared_result_releases_the_old_one() {
    let old = shared(10);
    let store = RowStore::from_shared(old.clone());
    store.replace_shared(shared(2));
    assert_eq!(Arc::strong_count(&old), 1);
    assert_eq!(store.n_items(), 2);
}

#[test]
fn projected_store_keeps_sql_null_distinct_from_an_unfetched_cell() {
    let result = Arc::new(QueryResult {
        columns: vec![column("id"), column("name")],
        rows: vec![vec![Value::Null, Value::Text("Ada".into())]],
        truncated: false,
    });
    let store = RowStore::from_projected(result.clone(), vec![0, 2], 3).unwrap();
    let row = store.item(0).and_downcast::<RowObject>().unwrap();

    assert!(row.cell_is_loaded(0));
    assert_eq!(row.cell_value(0), Value::Null);
    assert!(!row.cell_is_loaded(1));
    assert!(row.complete_cells().is_none());
    assert_eq!(row.cell_value(2), Value::Text("Ada".into()));

    store.replace_shared(shared(1));
    assert_eq!(row.cell_value(0), Value::Null);
    assert!(!row.cell_is_loaded(1));
    assert_eq!(row.cell_value(2), Value::Text("Ada".into()));
    assert_eq!(Arc::strong_count(&result), 1);
}

#[test]
fn search_visits_projected_cells_at_their_visible_columns() {
    let result = Arc::new(QueryResult {
        columns: vec![column("id"), column("name")],
        rows: vec![vec![Value::Int(7), Value::Text("Ada".into())]],
        truncated: false,
    });
    let store = RowStore::from_projected(result, vec![0, 2], 3).unwrap();

    let mut searched = Vec::new();
    store.visit_search_cells(0, |column, value| {
        searched.push((column, value.clone()));
        false
    });
    assert_eq!(searched, vec![(0, Value::Int(7)), (2, Value::Text("Ada".into()))]);
}

#[test]
fn projected_browse_rows_preview_long_values_without_losing_the_key() {
    let text = "x".repeat(9000);
    let result = Arc::new(QueryResult {
        columns: vec![column("id"), column("payload")],
        rows: vec![vec![Value::Int(7), Value::Text(text.clone())]],
        truncated: false,
    });
    let store = RowStore::from_projected_with_previews(result, vec![0, 2], 3, vec![0]).unwrap();
    let row = store.item(0).and_downcast::<RowObject>().unwrap();

    assert_eq!(row.cell_value(0), Value::Int(7));
    assert!(row.cell_preview(2).is_some());
    assert!(!row.cell_is_loaded(1));
    assert_eq!(row.cell_value(2), Value::Text(text.clone()));
    assert_eq!(row.cell_preview(2).unwrap().byte_count, text.len());
    assert!(
        row.complete_cells().is_none(),
        "the projected hidden column is still unfetched"
    );
}

#[test]
fn arbitrary_query_previews_can_recover_full_values_from_the_shared_result() {
    let text = "x".repeat(9000);
    let bytes = vec![7; 9000];
    let json = serde_json::json!({"payload": text});
    let result = Arc::new(QueryResult {
        columns: vec![column("id"), column("text"), column("bytes"), column("json")],
        rows: vec![vec![
            Value::Int(7),
            Value::Text("x".repeat(9000)),
            Value::Bytes(bytes.clone()),
            Value::Json(Box::new(json.clone())),
        ]],
        truncated: false,
    });
    let store = RowStore::from_shared_with_result_previews(result);
    let row = store.item(0).and_downcast::<RowObject>().unwrap();

    assert_eq!(row.cell_value(0), Value::Int(7));
    assert_eq!(row.cell_preview(1).unwrap().byte_count, 9000);
    assert_eq!(row.cell_preview(2).unwrap().byte_count, bytes.len());
    assert!(row.cell_preview(3).unwrap().byte_count > 9000);
    assert_eq!(store.source_value_at(0, 1), Some(Value::Text("x".repeat(9000))));
    assert_eq!(store.source_value_at(0, 2), Some(Value::Bytes(bytes)));
    assert_eq!(store.source_value_at(0, 3), Some(Value::Json(Box::new(json.clone()))));
    assert_eq!(
        row.complete_cells(),
        Some(vec![
            Value::Int(7),
            Value::Text("x".repeat(9000)),
            Value::Bytes(vec![7; 9000]),
            Value::Json(Box::new(json))
        ]),
        "copy, export and row JSON read the full values"
    );
}

#[test]
fn redis_previews_only_string_values_and_preserves_keys_and_other_types() {
    let long = "x".repeat(9000);
    let result = Arc::new(QueryResult {
        columns: vec![column("Key"), column("Type"), column("TTL"), column("Value")],
        rows: vec![
            vec![
                Value::Text("str-key".into()),
                Value::Text("string".into()),
                Value::Int(-1),
                Value::Text(long.clone()),
            ],
            vec![
                Value::Text("hash-key".into()),
                Value::Text("hash".into()),
                Value::Int(-1),
                Value::Text(long.clone()),
            ],
        ],
        truncated: false,
    });
    let store = RowStore::from_shared_with_redis_string_previews(result, vec![0]);
    let string_row = store.item(0).and_downcast::<RowObject>().unwrap();
    let hash_row = store.item(1).and_downcast::<RowObject>().unwrap();
    assert_eq!(string_row.cell_value(0), Value::Text("str-key".into()));
    assert_eq!(string_row.cell_preview(3).unwrap().byte_count, long.len());
    assert_eq!(hash_row.cell_value(3), Value::Text(long));
    assert!(hash_row.cell_preview(3).is_none());
}

#[test]
fn loaded_row_search_visits_full_shared_value_without_materializing_it_in_the_grid() {
    let text = "x".repeat(9000);
    let result = Arc::new(QueryResult {
        columns: vec![column("id"), column("payload")],
        rows: vec![vec![Value::Int(7), Value::Text(text.clone())]],
        truncated: false,
    });
    let store = RowStore::from_shared_with_previews(result, vec![0]);
    let row = store.item(0).and_downcast::<RowObject>().unwrap();

    assert!(row.cell_preview(1).is_some());
    assert_eq!(row.cell_value(1), Value::Text(text.clone()));
    let mut found_full_value = false;
    assert!(store.visit_search_cells(0, |index, value| {
        if index == 1 {
            found_full_value = matches!(value, Value::Text(value) if value == &text);
            true
        } else {
            false
        }
    }));
    assert!(found_full_value);
}

#[test]
fn search_visits_an_edited_cached_value() {
    let store = RowStore::from_shared(shared(1));
    let row = store.item(0).and_downcast::<RowObject>().unwrap();
    row.set_cell(0, Value::Text("edited".into()));

    let mut searched = None;
    assert!(store.visit_search_cells(0, |column, value| {
        searched = Some((column, value.clone()));
        true
    }));
    assert_eq!(searched, Some((0, Value::Text("edited".into()))));
}

#[test]
fn projected_store_refuses_rows_that_do_not_match_the_column_map() {
    let result = Arc::new(QueryResult {
        columns: vec![column("id")],
        rows: vec![vec![Value::Int(1), Value::Int(2)]],
        truncated: false,
    });
    assert!(RowStore::from_projected(result, vec![0], 1).is_err());
}

fn keyed_page(count: i64) -> Arc<QueryResult> {
    Arc::new(QueryResult {
        columns: vec![column("id"), column("payload")],
        rows: (0..count)
            .map(|n| vec![Value::Int(n), Value::Text("x".repeat(9000))])
            .collect(),
        truncated: false,
    })
}

fn persisted(id: i64) -> RowKey {
    RowKey::Persisted(vec![KeyValue::Int(id)])
}

#[test]
fn key_lookups_read_unbound_rows_without_building_row_objects() {
    let store = RowStore::from_shared_with_previews(keyed_page(200), vec![0]);
    store.insert(0, &RowObject::new_draft(7, vec![Value::Int(-1), Value::Null]));
    let held = store.item(6).and_downcast::<RowObject>().unwrap();

    assert_eq!(store.position_of_key(&persisted(150), &[0]), Some(151));
    assert_eq!(store.position_of_key(&persisted(5), &[0]), Some(6));
    assert_eq!(store.position_of_key(&RowKey::Draft(7), &[0]), Some(0));
    assert_eq!(store.position_of_key(&persisted(-1), &[0]), None);
    assert_eq!(store.position_of_key(&persisted(999), &[0]), None);
    assert_eq!(store.position_of_key(&persisted(5), &[]), None);
    let keys = (0..store.n_items())
        .map(|position| store.row_key_at(position, &[0]))
        .collect::<Vec<_>>();
    assert_eq!(keys[0], Some(RowKey::Draft(7)));
    assert_eq!(keys[6], Some(persisted(5)));
    assert_eq!(keys[200], Some(persisted(199)));
    assert_eq!(store.row_key_at(1, &[2]), None);

    assert_eq!(store.requested(), 1, "only the held row has a RowObject");
    drop(held);
}

#[test]
fn projected_key_lookups_map_columns_and_reject_unfetched_keys() {
    let result = Arc::new(QueryResult {
        columns: vec![column("id"), column("payload")],
        rows: (0..20).map(|n| vec![Value::Int(n), Value::Text("x".into())]).collect(),
        truncated: false,
    });
    let store = RowStore::from_projected(result, vec![0, 2], 3).unwrap();

    assert_eq!(store.position_of_key(&persisted(12), &[0]), Some(12));
    assert_eq!(store.row_key_at(3, &[0]), Some(persisted(3)));
    assert_eq!(store.row_key_at(3, &[1]), None);
    assert_eq!(store.row_key_at(3, &[3]), None);
    assert_eq!(store.requested(), 0);
}

fn column(name: &str) -> tablepro_core::ColumnInfo {
    tablepro_core::ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}
