use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use tablepro_core::{QueryResult, Value};

use crate::ui::row_store::RowStore;

use super::local_sort;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResultViewKey {
    pub(crate) statement_identity: crate::ui::editor::StatementIdentity,
    pub(crate) result_set_index: usize,
}

#[derive(Clone, Default)]
pub(crate) struct LocalResultSettingsStore(Rc<RefCell<HashMap<ResultViewKey, LocalResultSettings>>>);

#[derive(Clone, Default)]
pub(crate) struct LocalResultSettings {
    pub(crate) filter: Option<(usize, Value)>,
    pub(crate) sort: Option<(usize, bool)>,
    selection: Option<(std::sync::Weak<QueryResult>, HashSet<usize>)>,
}

impl LocalResultSettingsStore {
    fn get(&self, key: ResultViewKey) -> LocalResultSettings {
        self.0.borrow().get(&key).cloned().unwrap_or_default()
    }

    pub(crate) fn update(&self, key: ResultViewKey, update: impl FnOnce(&mut LocalResultSettings)) {
        update(self.0.borrow_mut().entry(key).or_default());
    }

    pub(crate) fn retain(&self, keys: &HashSet<ResultViewKey>) {
        self.0.borrow_mut().retain(|key, _| keys.contains(key));
    }

    fn selection_for(&self, key: ResultViewKey, result: &Arc<QueryResult>) -> Option<HashSet<usize>> {
        let state = self.0.borrow();
        let (saved_result, selected) = state.get(&key)?.selection.as_ref()?;
        let saved_result = saved_result.upgrade()?;
        Arc::ptr_eq(&saved_result, result).then(|| selected.clone())
    }
}

#[derive(Clone)]
pub(crate) struct LocalResultView(Rc<RefCell<LocalResultViewState>>);

struct LocalResultViewState {
    result: Arc<QueryResult>,
    store: Option<RowStore>,
    selection: Option<gtk4::MultiSelection>,
    filter: Option<(usize, Value)>,
    sort: Option<(usize, bool)>,
    controls: gtk4::Box,
    status: gtk4::Label,
    clear_button: gtk4::Button,
    settings_store: LocalResultSettingsStore,
    settings_key: ResultViewKey,
    selection_syncing: Rc<Cell<bool>>,
}

impl LocalResultView {
    pub(crate) fn new(
        result: Arc<QueryResult>,
        settings_store: LocalResultSettingsStore,
        settings_key: ResultViewKey,
    ) -> Self {
        let controls = gtk4::Box::builder()
            .spacing(6)
            .visible(false)
            .margin_start(8)
            .margin_end(8)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        let status = gtk4::Label::builder().hexpand(true).halign(gtk4::Align::Start).build();
        let clear = gtk4::Button::with_label(&crate::tr!("Clear filter"));
        controls.append(&status);
        controls.append(&clear);
        let settings = settings_store.get(settings_key);
        let view = Self(Rc::new(RefCell::new(LocalResultViewState {
            result,
            store: None,
            selection: None,
            filter: settings.filter,
            sort: settings.sort,
            controls,
            status,
            clear_button: clear.clone(),
            settings_store,
            settings_key,
            selection_syncing: Rc::new(Cell::new(false)),
        })));
        let weak_view = Rc::downgrade(&view.0);
        clear.connect_clicked(move |_| {
            if let Some(view) = weak_view.upgrade() {
                Self(view).clear_filter();
            }
        });
        view
    }

    pub(crate) fn has_filter(&self) -> bool {
        self.0.borrow().filter.is_some()
    }

    pub(crate) fn loaded_result(&self) -> Arc<QueryResult> {
        self.0.borrow().result.clone()
    }

    pub(crate) fn controls(&self) -> gtk4::Box {
        self.0.borrow().controls.clone()
    }

    pub(crate) fn attach(&self, store: RowStore, selection: gtk4::MultiSelection) {
        let (result, settings_store, settings_key, selection_syncing) = {
            let state = self.0.borrow();
            (
                state.result.clone(),
                state.settings_store.clone(),
                state.settings_key,
                state.selection_syncing.clone(),
            )
        };
        {
            let mut state = self.0.borrow_mut();
            state.store = Some(store.clone());
            state.selection = Some(selection.clone());
            update_status(&state);
        }
        self.rebuild_visible_rows();
        if let Some(selected) = settings_store.selection_for(settings_key, &result) {
            selection_syncing.set(true);
            let positions = gtk4::Bitset::new_empty();
            for position in 0..store.n_items() {
                if store
                    .source_index_at(position)
                    .is_some_and(|source| selected.contains(&source))
                {
                    positions.add(position);
                }
            }
            selection.set_selection(&positions, &gtk4::Bitset::new_range(0, store.n_items()));
            selection_syncing.set(false);
        }
        selection.connect_selection_changed(move |selection, _, _| {
            if selection_syncing.get() {
                return;
            }
            let selected = selected_source_rows(&store, selection);
            settings_store.update(settings_key, |settings| {
                settings.selection = Some((Arc::downgrade(&result), selected));
            });
        });
    }

    pub(crate) fn filter_by_value(&self, column: usize, value: Value) {
        if self.0.borrow().result.rows.iter().any(|row| row.get(column).is_some()) {
            self.0.borrow_mut().filter = Some((column, value));
            self.persist_settings();
            self.rebuild_visible_rows();
        }
    }

    pub(crate) fn clear_filter(&self) {
        self.0.borrow_mut().filter = None;
        self.persist_settings();
        self.rebuild_visible_rows();
    }

    pub(crate) fn sort_by(&self, column: usize, ascending: bool) {
        let result = self.0.borrow().result.clone();
        if local_sort::is_sortable_column(&result, column) {
            self.0.borrow_mut().sort = Some((column, ascending));
            self.persist_settings();
            self.rebuild_visible_rows();
        }
    }

    fn persist_settings(&self) {
        let state = self.0.borrow();
        state.settings_store.update(state.settings_key, |settings| {
            settings.filter = state.filter.clone();
            settings.sort = state.sort;
        });
    }

    fn rebuild_visible_rows(&self) {
        let (result, store, selection, filter, sort) = {
            let state = self.0.borrow();
            let Some(store) = state.store.clone() else { return };
            let Some(selection) = state.selection.clone() else {
                return;
            };
            (state.result.clone(), store, selection, state.filter.clone(), state.sort)
        };
        let selected = selected_source_rows(&store, &selection);
        let mut order = match sort {
            Some((column, ascending)) => local_sort::sorted_source_indices(&result, column, ascending)
                .unwrap_or_else(|| (0..result.rows.len()).collect()),
            None => (0..result.rows.len()).collect(),
        };
        if let Some((column, value)) = filter {
            order.retain(|source_index| {
                result
                    .rows
                    .get(*source_index)
                    .and_then(|row| row.get(column))
                    .is_some_and(|candidate| same_filter_value(candidate, &value))
            });
        }
        let syncing = self.0.borrow().selection_syncing.clone();
        syncing.set(true);
        if !store.replace_source_rows(&order) {
            syncing.set(false);
            return;
        }
        let positions = gtk4::Bitset::new_empty();
        for (position, source_index) in order.iter().enumerate() {
            if selected.contains(source_index) {
                positions.add(position as u32);
            }
        }
        selection.set_selection(&positions, &gtk4::Bitset::new_range(0, order.len() as u32));
        syncing.set(false);
        update_status(&self.0.borrow());
    }
}

fn selected_source_rows(store: &RowStore, selection: &gtk4::MultiSelection) -> std::collections::HashSet<usize> {
    (0..selection.n_items())
        .filter(|position| selection.is_selected(*position))
        .filter_map(|position| store.source_index_at(position))
        .collect()
}

fn update_status(state: &LocalResultViewState) {
    if state.filter.is_none() {
        state.clear_button.set_visible(false);
        state.controls.set_visible(state.result.truncated);
        if state.result.truncated {
            state.status.add_css_class("warning");
            state
                .status
                .set_text(&truncation_notice(state.sort.is_some(), state.result.rows.len()));
        }
        return;
    }
    state.clear_button.set_visible(true);
    let visible = state.store.as_ref().map_or(0, |store| store.n_items());
    state.status.set_text(&filter_summary(
        u128::from(visible),
        state.result.rows.len() as u128,
        state.result.truncated,
    ));
    if state.result.truncated {
        state.status.add_css_class("warning");
    } else {
        state.status.remove_css_class("warning");
    }
    state.controls.set_visible(true);
}

fn filter_summary(visible: u128, loaded: u128, truncated: bool) -> String {
    let mut summary = crate::tr!("Showing {visible} of {loaded} loaded rows")
        .replace("{visible}", &crate::ui::count_format::format_thousands(visible))
        .replace("{loaded}", &crate::ui::count_format::format_thousands(loaded));
    if truncated {
        summary.push_str(" · ");
        summary.push_str(&crate::tr!("result truncated"));
    }
    summary
}

fn truncation_notice(sorted: bool, loaded_rows: usize) -> String {
    if sorted {
        return crate::tr!("Sorted the loaded rows only; the result is truncated. Use ORDER BY for the full result.");
    }
    crate::tr!("Only the first {loaded} rows were loaded; the result is truncated. Narrow the query to see the rest.")
        .replace(
            "{loaded}",
            &crate::ui::count_format::format_thousands(loaded_rows as u128),
        )
}

fn same_filter_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Float(left), Value::Float(right)) => left == right || (left.is_nan() && right.is_nan()),
        _ => left == right,
    }
}

#[cfg(test)]
fn matching_source_indices(result: &QueryResult, column: usize, value: &Value) -> Vec<usize> {
    result
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            row.get(column)
                .is_some_and(|candidate| same_filter_value(candidate, value))
                .then_some(index)
        })
        .collect()
}

#[cfg(test)]
#[path = "local_filter_tests.rs"]
mod tests;
