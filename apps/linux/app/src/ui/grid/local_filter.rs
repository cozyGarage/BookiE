use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use bookie_core::{QueryResult, Value};
use gtk4::prelude::*;

use crate::ui::row_store::RowStore;

use super::local_sort;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResultViewKey {
    pub(crate) statement_identity: crate::ui::editor::StatementIdentity,
    pub(crate) result_set_index: usize,
}

#[derive(Clone, Default)]
pub(crate) struct LocalResultSettingsStore(Rc<RefCell<HashMap<ResultViewKey, LocalResultSettings>>>);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalCellFilter {
    pub(crate) column: usize,
    pub(crate) value: Value,
    pub(crate) exclude: bool,
}

#[derive(Clone, Default)]
pub(crate) struct LocalResultSettings {
    pub(crate) filter: Option<LocalCellFilter>,
    pub(crate) sort: Vec<(usize, bool)>,
    pub(crate) changed_cells: Option<Arc<bookie_app_services::result_compare::ChangedCells>>,
    selection: Option<(std::sync::Weak<QueryResult>, Arc<HashSet<usize>>)>,
}

impl LocalResultSettingsStore {
    pub(crate) fn changed_cells(
        &self,
        key: ResultViewKey,
    ) -> Option<Arc<bookie_app_services::result_compare::ChangedCells>> {
        self.0.borrow().get(&key)?.changed_cells.clone()
    }

    fn get(&self, key: ResultViewKey) -> LocalResultSettings {
        self.0.borrow().get(&key).cloned().unwrap_or_default()
    }

    pub(crate) fn update(&self, key: ResultViewKey, update: impl FnOnce(&mut LocalResultSettings)) {
        update(self.0.borrow_mut().entry(key).or_default());
    }

    pub(crate) fn retain(&self, keys: &HashSet<ResultViewKey>) {
        self.0.borrow_mut().retain(|key, _| keys.contains(key));
    }

    fn selection_for(&self, key: ResultViewKey, result: &Arc<QueryResult>) -> Option<Arc<HashSet<usize>>> {
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
    filter: Option<LocalCellFilter>,
    sort: Vec<(usize, bool)>,
    controls: gtk4::Box,
    status: gtk4::Label,
    clear_button: gtk4::Button,
    settings_store: LocalResultSettingsStore,
    settings_key: ResultViewKey,
    selection_syncing: Rc<Cell<bool>>,
    column_view: Option<glib::WeakRef<gtk4::ColumnView>>,
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
        let clear = gtk4::Button::with_label(&crate::tr!("Clear"));
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
            column_view: None,
        })));
        let weak_view = Rc::downgrade(&view.0);
        clear.connect_clicked(move |_| {
            if let Some(view) = weak_view.upgrade() {
                Self(view).clear_filter_and_sort();
            }
        });
        view
    }

    pub(crate) fn bind_column_view(&self, column_view: &gtk4::ColumnView) {
        self.0.borrow_mut().column_view = Some(column_view.downgrade());
    }

    pub(crate) fn statement_identity(&self) -> crate::ui::editor::StatementIdentity {
        self.0.borrow().settings_key.statement_identity
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
        let has_saved_view_order = {
            let state = self.0.borrow();
            state.filter.is_some() || !state.sort.is_empty()
        };
        if has_saved_view_order {
            self.rebuild_visible_rows();
        }
        if let Some(selected) = settings_store.selection_for(settings_key, &result) {
            restore_selection(
                &store,
                &selection,
                &selected,
                has_saved_view_order,
                result.rows.len(),
                &selection_syncing,
            );
        }
        selection.connect_selection_changed(move |selection, _, _| {
            if selection_syncing.get() {
                return;
            }
            let selected = selected_source_rows(&store, selection);
            settings_store.update(settings_key, |settings| {
                settings.selection = Some((Arc::downgrade(&result), Arc::new(selected)));
            });
        });
    }

    pub(crate) fn filter_by_value(&self, column: usize, value: Value, exclude: bool) -> bool {
        if bookie_core::equality_rule("_", &value).is_none() {
            return false;
        }
        if !self.0.borrow().result.rows.iter().any(|row| row.get(column).is_some()) {
            return false;
        }
        self.0.borrow_mut().filter = Some(LocalCellFilter { column, value, exclude });
        self.persist_settings();
        self.rebuild_visible_rows();
        true
    }

    #[cfg(test)]
    pub(crate) fn clear_filter(&self) {
        self.0.borrow_mut().filter = None;
        self.persist_settings();
        self.rebuild_visible_rows();
    }

    #[cfg(test)]
    pub(crate) fn clear_sort(&self) {
        let column_view = self.0.borrow().column_view.as_ref().and_then(glib::WeakRef::upgrade);
        if let Some(column_view) = column_view {
            column_view.sort_by_column(None::<&gtk4::ColumnViewColumn>, gtk4::SortType::Ascending);
        }
        if !self.0.borrow().sort.is_empty() {
            self.0.borrow_mut().sort.clear();
            self.persist_settings();
            self.rebuild_visible_rows();
        }
    }

    pub(crate) fn clear_filter_and_sort(&self) {
        let had_filter = self.0.borrow().filter.is_some();
        if had_filter {
            self.0.borrow_mut().filter = None;
            self.persist_settings();
        }
        let column_view = self.0.borrow().column_view.as_ref().and_then(glib::WeakRef::upgrade);
        if let Some(column_view) = column_view {
            column_view.sort_by_column(None::<&gtk4::ColumnViewColumn>, gtk4::SortType::Ascending);
        }
        let needs_rebuild = !self.0.borrow().sort.is_empty() || had_filter;
        if !self.0.borrow().sort.is_empty() {
            self.0.borrow_mut().sort.clear();
            self.persist_settings();
        }
        if needs_rebuild {
            self.rebuild_visible_rows();
        }
    }

    #[cfg(test)]
    pub(crate) fn sort_keys(&self) -> Vec<(usize, bool)> {
        self.0.borrow().sort.clone()
    }

    #[cfg(test)]
    pub(crate) fn sort_by(&self, column: usize, ascending: bool) {
        self.sort_by_keys(&[(column, ascending)]);
    }

    pub(crate) fn sort_by_keys(&self, keys: &[(usize, bool)]) {
        let keys = keys.to_vec();
        let (result, attached, filter) = {
            let state = self.0.borrow();
            (
                state.result.clone(),
                state.store.is_some() && state.selection.is_some(),
                state.filter.clone(),
            )
        };
        let sorted_order = if keys.is_empty() {
            None
        } else if !attached {
            if keys
                .iter()
                .any(|&(column, _)| !local_sort::is_sortable_column(&result, column))
            {
                return;
            }
            None
        } else if let Some(filter) = filter.as_ref() {
            let matches = matching_source_indices(&result, filter.column, &filter.value, filter.exclude);
            let Ok(order) = local_sort::sorted_source_indices_for_subset_by_keys(&result, &keys, matches) else {
                return;
            };
            Some(order)
        } else {
            let Some(order) = local_sort::sorted_source_indices_by_keys(&result, &keys) else {
                return;
            };
            Some(order)
        };
        self.0.borrow_mut().sort = keys;
        self.persist_settings();
        self.rebuild_visible_rows_with_sorted_order(sorted_order);
    }

    fn persist_settings(&self) {
        let state = self.0.borrow();
        state.settings_store.update(state.settings_key, |settings| {
            settings.filter = state.filter.clone();
            settings.sort.clone_from(&state.sort);
        });
    }

    fn rebuild_visible_rows(&self) {
        self.rebuild_visible_rows_with_sorted_order(None);
    }

    fn rebuild_visible_rows_with_sorted_order(&self, sorted_order: Option<Vec<usize>>) {
        let (result, store, selection, filter, sort) = {
            let state = self.0.borrow();
            let Some(store) = state.store.clone() else { return };
            let Some(selection) = state.selection.clone() else {
                return;
            };
            (
                state.result.clone(),
                store,
                selection,
                state.filter.clone(),
                state.sort.clone(),
            )
        };
        let selected = selected_source_rows(&store, &selection);
        let order = match filter {
            Some(filter) => {
                if let Some(order) = sorted_order {
                    order
                } else {
                    let matches = matching_source_indices(&result, filter.column, &filter.value, filter.exclude);
                    if sort.is_empty() {
                        matches
                    } else {
                        local_sort::sorted_source_indices_for_subset_by_keys(&result, &sort, matches)
                            .unwrap_or_else(|unsorted| unsorted)
                    }
                }
            }
            None => {
                if sort.is_empty() {
                    (0..result.rows.len()).collect()
                } else {
                    sorted_order
                        .or_else(|| local_sort::sorted_source_indices_by_keys(&result, &sort))
                        .unwrap_or_else(|| (0..result.rows.len()).collect())
                }
            }
        };
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

fn restore_selection(
    store: &RowStore,
    selection: &gtk4::MultiSelection,
    selected: &HashSet<usize>,
    has_saved_view_order: bool,
    source_row_count: usize,
    selection_syncing: &Cell<bool>,
) {
    selection_syncing.set(true);
    let item_count = store.n_items();
    let all_source_rows_selected = selected.len() == source_row_count;
    let positions = if all_source_rows_selected || (!has_saved_view_order && selected.len() == item_count as usize) {
        gtk4::Bitset::new_range(0, item_count)
    } else {
        let positions = gtk4::Bitset::new_empty();
        if !has_saved_view_order && selected.len().saturating_mul(4) < item_count as usize {
            for source in selected {
                if let Ok(position) = u32::try_from(*source)
                    && position < item_count
                {
                    positions.add(position);
                }
            }
        } else {
            for position in 0..item_count {
                if store
                    .source_index_at(position)
                    .is_some_and(|source| selected.contains(&source))
                {
                    positions.add(position);
                }
            }
        }
        positions
    };
    selection.set_selection(&positions, &gtk4::Bitset::new_range(0, item_count));
    selection_syncing.set(false);
}

fn selected_source_rows(store: &RowStore, selection: &gtk4::MultiSelection) -> std::collections::HashSet<usize> {
    let positions = selection.selection();
    let capacity = usize::try_from(positions.size()).unwrap_or_default();
    let mut source_rows = HashSet::with_capacity(capacity);
    let Some((mut iterator, mut position)) = gtk4::BitsetIter::init_first(&positions) else {
        return source_rows;
    };
    loop {
        if let Some(source_index) = store.source_index_at(position) {
            source_rows.insert(source_index);
        }
        let Some(next) = iterator.next() else { break };
        position = next;
    }
    source_rows
}

fn update_status(state: &LocalResultViewState) {
    let sorted = !state.sort.is_empty();
    let filtered = state.filter.is_some();
    let truncated = state.result.truncated;
    if !filtered && !sorted && !truncated {
        state.clear_button.set_visible(false);
        state.controls.set_visible(false);
        return;
    }
    state.clear_button.set_visible(filtered || sorted);
    state.clear_button.set_label(&clear_button_label(filtered, sorted));
    let sort_summary = sort_keys_summary(&state.result, &state.sort);
    if !filtered {
        if truncated {
            state.status.add_css_class("warning");
            let mut text = truncation_notice(sorted, state.result.rows.len());
            if let Some(summary) = sort_summary {
                text = format!("{summary} · {text}");
            }
            state.status.set_text(&text);
        } else if let Some(summary) = sort_summary {
            state.status.remove_css_class("warning");
            state.status.set_text(&summary);
        }
        state.controls.set_visible(true);
        return;
    }
    let visible = state.store.as_ref().map_or(0, |store| store.n_items());
    let mut summary = filter_summary(
        u128::from(visible),
        state.result.rows.len() as u128,
        state.result.truncated,
    );
    if let Some(sort_summary) = sort_summary {
        summary = format!("{sort_summary} · {summary}");
    }
    state.status.set_text(&summary);
    if truncated {
        state.status.add_css_class("warning");
    } else {
        state.status.remove_css_class("warning");
    }
    state.controls.set_visible(true);
}

fn clear_button_label(filtered: bool, sorted: bool) -> String {
    match (filtered, sorted) {
        (true, false) => crate::tr!("Clear filter"),
        (false, true) => crate::tr!("Clear sort"),
        _ => crate::tr!("Clear"),
    }
}

fn sort_keys_summary(result: &QueryResult, keys: &[(usize, bool)]) -> Option<String> {
    if keys.is_empty() {
        return None;
    }
    let parts = keys
        .iter()
        .map(|&(column, ascending)| {
            let name = result
                .columns
                .get(column)
                .map(|column| column.name.as_str())
                .unwrap_or("?");
            let arrow = if ascending { "↑" } else { "↓" };
            format!("{name} {arrow}")
        })
        .collect::<Vec<_>>();
    Some(crate::tr!("Sorted by {keys}").replace("{keys}", &parts.join(", ")))
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

fn matching_source_indices(result: &QueryResult, column: usize, value: &Value, exclude: bool) -> Vec<usize> {
    result
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            row.get(column)
                .is_some_and(|candidate| {
                    let matches = same_filter_value(candidate, value);
                    if exclude { !matches } else { matches }
                })
                .then_some(index)
        })
        .collect()
}

#[cfg(test)]
#[path = "local_filter_tests.rs"]
mod tests;
