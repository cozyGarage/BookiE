use std::sync::Arc;

use gtk4::prelude::*;
use tablepro_core::{ColumnInfo, QueryResult, Value};

use super::{
    LocalResultSettingsStore, LocalResultView, ResultViewKey, filter_summary, matching_source_indices,
    truncation_notice,
};

#[test]
fn large_local_result_counts_use_the_same_grouping_as_the_paginator() {
    assert_eq!(filter_summary(1, 215_804, false), "Showing 1 of 215,804 loaded rows");
    assert_eq!(
        filter_summary(1_000, 215_804, true),
        "Showing 1,000 of 215,804 loaded rows · result truncated"
    );
}

#[test]
fn an_unsorted_truncated_result_states_how_many_rows_were_loaded() {
    let notice = truncation_notice(false, 215_804);
    assert!(notice.contains("215,804"), "{notice}");
    assert!(notice.contains("truncated"), "{notice}");
    assert!(truncation_notice(true, 215_804).contains("ORDER BY"));
}

fn result(values: Vec<Value>) -> QueryResult {
    QueryResult {
        columns: vec![ColumnInfo {
            name: "value".into(),
            data_type: "".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }],
        rows: values.into_iter().map(|value| vec![value]).collect(),
        truncated: false,
    }
}

#[test]
fn local_value_filter_matches_native_values_without_display_coercion() {
    let result = result(vec![
        Value::Text("1".into()),
        Value::Int(1),
        Value::Null,
        Value::Text("NULL".into()),
    ]);

    assert_eq!(matching_source_indices(&result, 0, &Value::Int(1)), vec![1]);
    assert_eq!(matching_source_indices(&result, 0, &Value::Null), vec![2]);
}

#[test]
fn local_value_filter_preserves_duplicate_source_order_and_ignores_missing_cells() {
    let mut result = result(vec![Value::Int(7), Value::Int(2), Value::Int(7)]);
    result.rows[1].clear();

    assert_eq!(matching_source_indices(&result, 0, &Value::Int(7)), vec![0, 2]);
    assert!(matching_source_indices(&result, 1, &Value::Int(7)).is_empty());
}

#[test]
fn local_value_filter_distinguishes_sql_null_from_json_null() {
    let result = result(vec![Value::Null, Value::Json(Box::new(serde_json::Value::Null))]);

    assert_eq!(matching_source_indices(&result, 0, &Value::Null), vec![0]);
    assert_eq!(
        matching_source_indices(&result, 0, &Value::Json(Box::new(serde_json::Value::Null))),
        vec![1]
    );
}

#[test]
fn local_value_filter_matches_nan_without_formatting_or_coercion() {
    let result = result(vec![Value::Float(f64::NAN), Value::Float(1.0)]);

    assert_eq!(matching_source_indices(&result, 0, &Value::Float(f64::NAN)), vec![0]);
}

#[test]
fn local_result_settings_survive_rerender_and_are_pruned_when_no_longer_visible() {
    let settings = LocalResultSettingsStore::default();
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 1,
            digest: [7; 32],
        },
        result_set_index: 0,
    };
    let second_set = ResultViewKey {
        result_set_index: 1,
        ..key
    };
    settings.update(key, |current| current.sort = Some((2, false)));
    settings.update(second_set, |current| current.sort = Some((0, true)));

    assert_eq!(settings.get(key).sort, Some((2, false)));
    settings.retain(&std::collections::HashSet::from([second_set]));
    assert_eq!(settings.get(key).sort, None);
    assert_eq!(settings.get(second_set).sort, Some((0, true)));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn local_filter_and_sort_keep_selection_attached_to_the_source_row() {
    gtk4::init().unwrap();
    let mut result = result(vec![Value::Int(3), Value::Int(1), Value::Int(2), Value::Int(2)]);
    result.truncated = true;
    let result = Arc::new(result);
    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let settings_store = LocalResultSettingsStore::default();
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [1; 32],
        },
        result_set_index: 0,
    };
    let view = LocalResultView::new(result.clone(), settings_store.clone(), key);
    view.attach(store.clone(), selection.clone());
    assert!(
        view.0.borrow().controls.is_visible(),
        "a truncated result discloses it before any sort or filter"
    );
    selection.select_item(2, true);

    view.filter_by_value(0, Value::Int(2));
    view.sort_by(0, false);

    assert_eq!(
        (0..store.n_items())
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(2), Some(3)]
    );
    let selected = selection.selection();
    assert_eq!(selected.size(), 1);
    assert_eq!(store.source_index_at(selected.nth(0)), Some(2));
    let state = view.0.borrow();
    assert!(state.controls.is_visible());
    assert!(state.status.has_css_class("warning"));
    drop(state);

    let rerun_store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let rerun_selection = gtk4::MultiSelection::new(Some(rerun_store.clone()));
    let rerun_view = LocalResultView::new(result.clone(), settings_store.clone(), key);
    rerun_view.attach(rerun_store.clone(), rerun_selection.clone());
    assert_eq!(rerun_store.n_items(), 2);
    assert_eq!(rerun_selection.selection().size(), 1);
    assert_eq!(rerun_store.source_index_at(rerun_selection.selection().nth(0)), Some(2));
    assert!(rerun_view.0.borrow().controls.is_visible());

    view.clear_filter();

    assert_eq!(store.n_items(), 4);
    assert_eq!(store.source_index_at(selection.selection().nth(0)), Some(2));
    let state = view.0.borrow();
    assert!(
        state.controls.is_visible(),
        "the sort of a truncated result keeps its notice"
    );
    assert!(!state.clear_button.is_visible());
}

#[test]
#[ignore = "requires isolated GTK display"]
fn filtering_a_fully_selected_result_restores_the_selection_in_one_signal() {
    gtk4::init().unwrap();
    let result = Arc::new(result((0..2000_i64).map(|value| Value::Int(value % 2)).collect()));
    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [2; 32],
        },
        result_set_index: 0,
    };
    let view = LocalResultView::new(result.clone(), LocalResultSettingsStore::default(), key);
    view.attach(store.clone(), selection.clone());
    selection.select_all();
    let emissions = std::rc::Rc::new(std::cell::Cell::new(0_u32));
    let counter = emissions.clone();
    selection.connect_selection_changed(move |_, _, _| counter.set(counter.get() + 1));

    view.filter_by_value(0, Value::Int(1));

    assert_eq!(store.n_items(), 1000);
    assert_eq!(selection.selection().size(), 1000);
    assert!(emissions.get() <= 2, "{} selection-changed emissions", emissions.get());
}

#[test]
#[ignore = "requires isolated GTK display"]
fn sorting_a_truncated_result_without_a_filter_warns_that_only_loaded_rows_were_sorted() {
    gtk4::init().unwrap();
    let mut loaded = result(vec![Value::Int(3), Value::Int(1), Value::Int(2)]);
    loaded.truncated = true;
    let loaded = Arc::new(loaded);
    let store = crate::ui::row_store::RowStore::from_shared(loaded.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [2; 32],
        },
        result_set_index: 0,
    };
    let view = LocalResultView::new(loaded.clone(), LocalResultSettingsStore::default(), key);
    view.attach(store, selection);
    assert!(view.0.borrow().controls.is_visible());

    view.sort_by(0, false);

    let state = view.0.borrow();
    assert!(state.controls.is_visible());
    assert!(state.status.has_css_class("warning"));
    assert!(!state.clear_button.is_visible());
    drop(state);

    let complete = Arc::new(result(vec![Value::Int(3), Value::Int(1)]));
    let complete_store = crate::ui::row_store::RowStore::from_shared(complete.clone());
    let complete_selection = gtk4::MultiSelection::new(Some(complete_store.clone()));
    let complete_view = LocalResultView::new(complete, LocalResultSettingsStore::default(), key);
    complete_view.attach(complete_store, complete_selection);
    complete_view.sort_by(0, true);
    assert!(!complete_view.0.borrow().controls.is_visible());
}
