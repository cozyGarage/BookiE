use std::sync::Arc;

use bookie_core::{ColumnInfo, QueryResult, Value};
use gtk4::prelude::*;

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

    assert_eq!(matching_source_indices(&result, 0, &Value::Int(1), false), vec![1]);
    assert_eq!(matching_source_indices(&result, 0, &Value::Null, false), vec![2]);
    assert_eq!(matching_source_indices(&result, 0, &Value::Int(1), true), vec![0, 2, 3]);
    assert_eq!(matching_source_indices(&result, 0, &Value::Null, true), vec![0, 1, 3]);
}

#[test]
fn local_exclude_keeps_null_and_empty_text_distinct() {
    let result = result(vec![Value::Null, Value::Text(String::new()), Value::Text("x".into())]);

    assert_eq!(matching_source_indices(&result, 0, &Value::Null, false), vec![0]);
    assert_eq!(
        matching_source_indices(&result, 0, &Value::Text(String::new()), false),
        vec![1]
    );
    assert_eq!(matching_source_indices(&result, 0, &Value::Null, true), vec![1, 2]);
    assert_eq!(
        matching_source_indices(&result, 0, &Value::Text(String::new()), true),
        vec![0, 2]
    );
}

#[test]
fn local_value_filter_preserves_duplicate_source_order_and_ignores_missing_cells() {
    let mut result = result(vec![Value::Int(7), Value::Int(2), Value::Int(7)]);
    result.rows.truncate_row(1, 0);

    assert_eq!(matching_source_indices(&result, 0, &Value::Int(7), false), vec![0, 2]);
    assert!(matching_source_indices(&result, 1, &Value::Int(7), false).is_empty());
}

#[test]
fn local_value_filter_distinguishes_sql_null_from_json_null() {
    let result = result(vec![Value::Null, Value::Json(Box::new(serde_json::Value::Null))]);

    assert_eq!(matching_source_indices(&result, 0, &Value::Null, false), vec![0]);
    assert_eq!(
        matching_source_indices(&result, 0, &Value::Json(Box::new(serde_json::Value::Null)), false),
        vec![1]
    );
}

#[test]
fn local_value_filter_matches_nan_without_formatting_or_coercion() {
    let result = result(vec![Value::Float(f64::NAN), Value::Float(1.0)]);

    assert_eq!(
        matching_source_indices(&result, 0, &Value::Float(f64::NAN), false),
        vec![0]
    );
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
    settings.update(key, |current| current.sort = vec![(2, false)]);
    settings.update(second_set, |current| current.sort = vec![(0, true)]);

    assert_eq!(settings.get(key).sort, vec![(2, false)]);
    settings.retain(&std::collections::HashSet::from([second_set]));
    assert_eq!(settings.get(key).sort, Vec::<(usize, bool)>::new());
    assert_eq!(settings.get(second_set).sort, vec![(0, true)]);
}

#[test]
#[ignore = "requires isolated GTK display"]
fn attaching_an_unfiltered_result_keeps_the_existing_row_model() {
    gtk4::init().unwrap();
    let result = Arc::new(result(vec![Value::Int(3), Value::Int(1), Value::Int(2)]));
    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let changes = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = changes.clone();
    store.connect_items_changed(move |_, _, _, _| observed.set(observed.get() + 1));
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [3; 32],
        },
        result_set_index: 0,
    };
    let view = LocalResultView::new(result.clone(), LocalResultSettingsStore::default(), key);

    view.attach(store.clone(), selection);

    assert_eq!(changes.get(), 0);
    assert_eq!(store.n_items(), 3);
    assert_eq!(
        (0..store.n_items())
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2)]
    );
}

#[test]
#[ignore = "requires isolated GTK display"]
fn attaching_an_unfiltered_result_restores_saved_selection_by_source_row() {
    gtk4::init().unwrap();
    let result = Arc::new(result(vec![Value::Int(3), Value::Int(1), Value::Int(2), Value::Int(4)]));
    let settings = LocalResultSettingsStore::default();
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [5; 32],
        },
        result_set_index: 0,
    };
    let initial_store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let initial_selection = gtk4::MultiSelection::new(Some(initial_store.clone()));
    let initial_view = LocalResultView::new(result.clone(), settings.clone(), key);
    initial_view.attach(initial_store, initial_selection.clone());
    initial_selection.select_item(0, false);
    initial_selection.select_item(2, false);

    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let changes = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = changes.clone();
    store.connect_items_changed(move |_, _, _, _| observed.set(observed.get() + 1));
    let view = LocalResultView::new(result, settings, key);
    view.attach(store.clone(), selection.clone());

    assert_eq!(changes.get(), 0);
    let restored = selection.selection();
    assert_eq!(restored.size(), 2);
    assert_eq!(store.source_index_at(restored.nth(0)), Some(0));
    assert_eq!(store.source_index_at(restored.nth(1)), Some(2));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn attaching_a_sorted_result_restores_selection_by_source_row() {
    gtk4::init().unwrap();
    let result = Arc::new(result(vec![Value::Int(3), Value::Int(1), Value::Int(2), Value::Int(4)]));
    let settings = LocalResultSettingsStore::default();
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [8; 32],
        },
        result_set_index: 0,
    };
    let selected = std::collections::HashSet::from([0, 2]);
    settings.update(key, |current| {
        current.sort = vec![(0, false)];
        current.selection = Some((Arc::downgrade(&result), super::SavedSelection::Rows(Arc::new(selected))));
    });

    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let view = LocalResultView::new(result, settings, key);
    view.attach(store.clone(), selection.clone());

    assert_eq!(
        (0..store.n_items())
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(3), Some(0), Some(2), Some(1)]
    );
    let restored = selection.selection();
    assert_eq!(restored.size(), 2);
    assert_eq!(store.source_index_at(restored.nth(0)), Some(0));
    assert_eq!(store.source_index_at(restored.nth(1)), Some(2));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn attaching_a_sorted_result_restores_all_source_rows_as_selected() {
    gtk4::init().unwrap();
    let result = Arc::new(result(vec![Value::Int(3), Value::Int(1), Value::Int(2), Value::Int(4)]));
    let settings = LocalResultSettingsStore::default();
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [10; 32],
        },
        result_set_index: 0,
    };
    settings.update(key, |current| {
        current.sort = vec![(0, false)];
        current.selection = Some((Arc::downgrade(&result), super::SavedSelection::All));
    });

    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let view = LocalResultView::new(result, settings, key);
    view.attach(store, selection.clone());

    assert_eq!(selection.selection().size(), 4);
}

#[test]
#[ignore = "requires isolated GTK display"]
fn attaching_a_filtered_result_restores_all_visible_selected_source_rows() {
    gtk4::init().unwrap();
    let result = Arc::new(result(vec![Value::Int(3), Value::Int(1), Value::Int(2), Value::Int(4)]));
    let settings = LocalResultSettingsStore::default();
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [11; 32],
        },
        result_set_index: 0,
    };
    settings.update(key, |current| {
        current.filter = Some(super::LocalCellFilter {
            column: 0,
            value: Value::Int(2),
            exclude: false,
        });
        current.selection = Some((Arc::downgrade(&result), super::SavedSelection::All));
    });

    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let view = LocalResultView::new(result, settings, key);
    view.attach(store.clone(), selection.clone());

    assert_eq!(store.n_items(), 1);
    assert_eq!(store.source_index_at(0), Some(2));
    assert_eq!(selection.selection().size(), 1);
    assert_eq!(store.source_index_at(selection.selection().nth(0)), Some(2));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn attaching_an_unfiltered_result_restores_the_entire_selection() {
    gtk4::init().unwrap();
    let result = Arc::new(result(vec![Value::Int(3), Value::Int(1), Value::Int(2), Value::Int(4)]));
    let settings = LocalResultSettingsStore::default();
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [6; 32],
        },
        result_set_index: 0,
    };
    let initial_store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let initial_selection = gtk4::MultiSelection::new(Some(initial_store.clone()));
    let initial_view = LocalResultView::new(result.clone(), settings.clone(), key);
    initial_view.attach(initial_store, initial_selection.clone());
    let all_positions = gtk4::Bitset::new_range(0, 4);
    initial_selection.set_selection(&all_positions, &all_positions);

    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let view = LocalResultView::new(result, settings, key);
    view.attach(store.clone(), selection.clone());

    let restored = selection.selection();
    assert_eq!(restored.size(), 4);
    assert_eq!(
        (0..restored.size() as u32)
            .map(|index| store.source_index_at(restored.nth(index)))
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2), Some(3)]
    );
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

    assert!(view.filter_by_value(0, Value::Int(2), false));
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
    assert!(state.clear_button.is_visible());
    assert_eq!(state.clear_button.label().as_deref(), Some("Clear sort"));
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

    assert!(view.filter_by_value(0, Value::Int(1), false));

    assert_eq!(store.n_items(), 1000);
    assert_eq!(selection.selection().size(), 1000);
    assert!(emissions.get() <= 2, "{} selection-changed emissions", emissions.get());
}

#[test]
#[ignore = "requires isolated GTK display"]
fn sorting_a_fully_selected_result_keeps_all_source_rows_selected() {
    gtk4::init().unwrap();
    let result = Arc::new(result(vec![Value::Int(3), Value::Int(1), Value::Int(2)]));
    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [8; 32],
        },
        result_set_index: 0,
    };
    let view = LocalResultView::new(result, LocalResultSettingsStore::default(), key);
    view.attach(store.clone(), selection.clone());
    selection.select_all();

    view.sort_by(0, true);

    assert_eq!(
        (0..store.n_items())
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(0)]
    );
    let restored = selection.selection();
    assert_eq!(restored.size(), 3);
    assert_eq!(
        (0..restored.size() as u32)
            .map(|index| store.source_index_at(restored.nth(index)))
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(0)]
    );
}

#[test]
#[ignore = "requires isolated GTK display"]
fn clearing_a_filter_does_not_select_rows_that_were_hidden() {
    gtk4::init().unwrap();
    let result = Arc::new(result(vec![Value::Int(1), Value::Int(2), Value::Int(1), Value::Int(2)]));
    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [9; 32],
        },
        result_set_index: 0,
    };
    let view = LocalResultView::new(result, LocalResultSettingsStore::default(), key);
    view.attach(store.clone(), selection.clone());
    assert!(view.filter_by_value(0, Value::Int(1), false));
    selection.select_all();

    view.clear_filter();

    let restored = selection.selection();
    assert_eq!(restored.size(), 2);
    assert_eq!(
        (0..restored.size() as u32)
            .map(|index| store.source_index_at(restored.nth(index)))
            .collect::<Vec<_>>(),
        vec![Some(0), Some(2)]
    );
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
    assert!(state.clear_button.is_visible());
    assert_eq!(state.clear_button.label().as_deref(), Some("Clear sort"));
    drop(state);

    let complete = Arc::new(result(vec![Value::Int(3), Value::Int(1)]));
    let complete_store = crate::ui::row_store::RowStore::from_shared(complete.clone());
    let complete_selection = gtk4::MultiSelection::new(Some(complete_store.clone()));
    let complete_view = LocalResultView::new(complete, LocalResultSettingsStore::default(), key);
    complete_view.attach(complete_store, complete_selection);
    complete_view.sort_by(0, true);
    assert!(complete_view.0.borrow().controls.is_visible());
    complete_view.clear_sort();
    assert!(!complete_view.0.borrow().controls.is_visible());
}

#[test]
#[ignore = "requires isolated GTK display"]
fn local_multi_key_sort_reverses_secondary_and_clears_to_source_order() {
    gtk4::init().unwrap();
    let result = Arc::new(QueryResult {
        columns: vec![
            ColumnInfo {
                name: "dept".into(),
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
            },
            ColumnInfo {
                name: "name".into(),
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
            },
        ],
        rows: vec![
            vec![Value::Text("b".into()), Value::Text("z".into())],
            vec![Value::Text("a".into()), Value::Text("y".into())],
            vec![Value::Text("a".into()), Value::Text("x".into())],
            vec![Value::Text("b".into()), Value::Text("w".into())],
        ]
        .into(),
        truncated: false,
    });
    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [12; 32],
        },
        result_set_index: 0,
    };
    let view = LocalResultView::new(result, LocalResultSettingsStore::default(), key);
    view.attach(store.clone(), selection);

    view.sort_by_keys(&[(0, true)]);
    assert_eq!(
        (0..store.n_items())
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(0), Some(3)]
    );
    view.sort_by_keys(&[(0, false)]);
    assert_eq!(
        (0..store.n_items())
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(0), Some(3), Some(1), Some(2)]
    );
    view.sort_by_keys(&[(0, true), (1, true)]);
    assert_eq!(view.sort_keys(), vec![(0, true), (1, true)]);
    assert_eq!(
        (0..store.n_items())
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(2), Some(1), Some(3), Some(0)]
    );
    let state = view.0.borrow();
    assert!(state.controls.is_visible());
    assert!(state.status.label().contains("dept"));
    assert!(state.status.label().contains("name"));
    drop(state);
    view.clear_sort();
    assert!(view.sort_keys().is_empty());
    assert_eq!(
        (0..store.n_items())
            .map(|position| store.source_index_at(position))
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1), Some(2), Some(3)]
    );
}

#[test]
#[ignore = "requires isolated GTK display"]
fn large_sort_replacement_keeps_newest_order_and_selection() {
    gtk4::init().unwrap();
    let result = Arc::new(result((0..20_000).map(Value::Int).collect()));
    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let key = ResultViewKey {
        statement_identity: crate::ui::editor::StatementIdentity {
            statement_index: 0,
            digest: [9; 32],
        },
        result_set_index: 0,
    };
    let settings = LocalResultSettingsStore::default();
    let view = LocalResultView::new(result.clone(), settings.clone(), key);
    view.attach(store.clone(), selection.clone());
    selection.select_item(123, false);
    view.sort_by(0, true);
    let first = view.0.borrow().pending.clone().unwrap();
    view.sort_by(0, false);
    assert!(first.load(std::sync::atomic::Ordering::Relaxed));
    let context = gtk4::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while view.0.borrow().pending.is_some() && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(view.0.borrow().pending.is_none(), "worker timed out");
    assert_eq!(store.source_index_at(0), Some(19_999));
    let selected = selection.selection();
    assert_eq!(selected.size(), 1);
    assert_eq!(store.source_index_at(selected.nth(0)), Some(123));
    let restored_store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let restored_selection = gtk4::MultiSelection::new(Some(restored_store.clone()));
    let restored_view = LocalResultView::new(result.clone(), settings, key);
    restored_view.attach(restored_store.clone(), restored_selection.clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while restored_view.0.borrow().pending.is_some() && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(restored_view.0.borrow().pending.is_none(), "restore worker timed out");
    assert_eq!(restored_store.source_index_at(0), Some(19_999));
    let restored = restored_selection.selection();
    assert_eq!(restored.size(), 1);
    assert_eq!(restored_store.source_index_at(restored.nth(0)), Some(123));
    assert!(view.filter_by_value(0, Value::Int(42), false));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while view.0.borrow().pending.is_some() && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(view.0.borrow().pending.is_none(), "filter worker timed out");
    assert_eq!(store.n_items(), 1);
    assert_eq!(store.source_index_at(0), Some(42));
    selection.select_all();
    view.clear_filter();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while view.0.borrow().pending.is_some() && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(view.0.borrow().pending.is_none(), "clear worker timed out");
    assert_eq!(store.n_items(), 20_000);
    let selected = selection.selection();
    assert_eq!(selected.size(), 1);
    assert_eq!(store.source_index_at(selected.nth(0)), Some(42));
    view.sort_by(0, true);
    let pending = view.0.borrow().pending.clone().unwrap();
    let weak = std::rc::Rc::downgrade(&view.0);
    drop(view);
    assert!(weak.upgrade().is_none(), "worker must not retain the view");
    assert!(pending.load(std::sync::atomic::Ordering::Relaxed));

    let failed_store = crate::ui::row_store::RowStore::from_shared(result.clone());
    failed_store.insert(0, &crate::ui::row_object::RowObject::new_draft(1, vec![Value::Int(-1)]));
    let failed_selection = gtk4::MultiSelection::new(Some(failed_store.clone()));
    let failed_view = LocalResultView::new(result.clone(), LocalResultSettingsStore::default(), key);
    failed_view.attach(failed_store.clone(), failed_selection.clone());
    failed_selection.select_item(24, false);
    let order = crate::ui::row_store::PreparedSourceRows::checked(
        result.clone(),
        (0..20_000).rev().collect(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert!(!failed_view.apply_prepared_order(&order));
    assert_eq!(failed_store.source_index_at(24), Some(23));
    let selected = failed_selection.selection();
    assert_eq!(selected.size(), 1);
    assert_eq!(selected.nth(0), 24);
    assert_eq!(failed_store.source_index_at(selected.nth(0)), Some(23));
    failed_selection.select_all();
    let selected_count = failed_selection.selection().size();
    assert!(!failed_view.apply_prepared_order(&order));
    assert_eq!(failed_selection.selection().size(), selected_count);

    let all_store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let all_selection = gtk4::MultiSelection::new(Some(all_store.clone()));
    let all_view = LocalResultView::new(result.clone(), LocalResultSettingsStore::default(), key);
    all_view.attach(all_store.clone(), all_selection.clone());
    all_selection.select_all();
    let reversed = crate::ui::row_store::PreparedSourceRows::checked(
        result,
        (0..20_000).rev().collect(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert!(all_view.apply_prepared_order(&reversed));
    let all_selected = all_selection.selection();
    assert_eq!(all_selected.size(), 20_000);
    assert!(all_selected.contains(0));
    assert!(all_selected.contains(19_999));
    assert_eq!(all_store.source_index_at(0), Some(19_999));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn million_row_select_all_sort_application_profile() {
    gtk4::init().unwrap();
    let row_count = 1_000_000usize;
    let result = Arc::new(result((0..row_count).map(|value| Value::Int(value as i64)).collect()));
    let store = crate::ui::row_store::RowStore::from_shared(result.clone());
    let selection = gtk4::MultiSelection::new(Some(store.clone()));
    let view = LocalResultView::new(
        result.clone(),
        LocalResultSettingsStore::default(),
        ResultViewKey {
            statement_identity: crate::ui::editor::StatementIdentity {
                statement_index: 0,
                digest: [10; 32],
            },
            result_set_index: 0,
        },
    );
    view.attach(store.clone(), selection.clone());
    selection.select_all();
    let order = crate::ui::row_store::PreparedSourceRows::checked(
        result,
        (0..row_count).rev().collect(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let started = std::time::Instant::now();
    assert!(view.apply_prepared_order(&order));
    eprintln!("million-row select-all apply: {:?}", started.elapsed());
    assert_eq!(selection.selection().size(), row_count as u64);
    assert_eq!(store.source_index_at(0), Some(row_count - 1));
}
