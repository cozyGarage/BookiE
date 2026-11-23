use crate::ui::spacing;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bookie_core::{QueryResult, Value};
use gtk4::prelude::*;

use crate::ui::row_store::{PreparedSourceRows, RowStore};

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

#[derive(Clone)]
enum SavedSelection {
    All,
    Rows(Arc<HashSet<usize>>),
}

#[derive(Clone, Default)]
pub(crate) struct LocalResultSettings {
    pub(crate) filter: Option<LocalCellFilter>,
    pub(crate) sort: Vec<(usize, bool)>,
    pub(crate) changed_cells: Option<Arc<bookie_app_services::result_compare::ChangedCells>>,
    selection: Option<(std::sync::Weak<QueryResult>, SavedSelection)>,
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

    fn selection_for(&self, key: ResultViewKey, result: &Arc<QueryResult>) -> Option<SavedSelection> {
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
    pending: Option<Arc<AtomicBool>>,
}

impl Drop for LocalResultViewState {
    fn drop(&mut self) {
        if let Some(cancel) = self.pending.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl LocalResultView {
    pub(crate) fn new(
        result: Arc<QueryResult>,
        settings_store: LocalResultSettingsStore,
        settings_key: ResultViewKey,
    ) -> Self {
        let controls = gtk4::Box::builder()
            .spacing(spacing::SM)
            .visible(false)
            .margin_start(spacing::SM)
            .margin_end(spacing::SM)
            .margin_top(spacing::XS)
            .margin_bottom(spacing::XS)
            .build();
        let status = gtk4::Label::builder().hexpand(true).halign(gtk4::Align::Start).build();
        let clear = gtk4::Button::with_label(&bookie_app_services::tr!("Clear"));
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
            pending: None,
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
                has_saved_view_order && self.0.borrow().pending.is_none(),
                result.rows.len(),
                &selection_syncing,
            );
        }
        selection.connect_selection_changed(move |selection, _, _| {
            if selection_syncing.get() {
                return;
            }
            let positions = selection.selection();
            let selected =
                if store.n_items() as usize == result.rows.len() && positions.size() == u64::from(store.n_items()) {
                    SavedSelection::All
                } else {
                    SavedSelection::Rows(Arc::new(selected_source_rows(&store, selection)))
                };
            settings_store.update(settings_key, |settings| {
                settings.selection = Some((Arc::downgrade(&result), selected));
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
        self.cancel_pending();
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
        if attached && result.rows.len() >= 10_000 {
            self.start_background_order(keys);
            return;
        }
        self.cancel_pending();
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
        let (large, keys) = {
            let state = self.0.borrow();
            (
                state.result.rows.len() >= 10_000 && state.store.is_some(),
                state.sort.clone(),
            )
        };
        if large {
            self.start_background_order(keys);
        } else {
            self.cancel_pending();
            self.rebuild_visible_rows_with_sorted_order(None);
        }
    }

    fn cancel_pending(&self) {
        let mut state = self.0.borrow_mut();
        if let Some(cancel) = state.pending.take() {
            cancel.store(true, Ordering::Relaxed);
            update_status(&state);
        }
    }

    fn start_background_order(&self, keys: Vec<(usize, bool)>) {
        self.cancel_pending();
        let cancel = Arc::new(AtomicBool::new(false));
        let (result, filter) = {
            let mut state = self.0.borrow_mut();
            state.pending = Some(cancel.clone());
            state.status.set_text(&bookie_app_services::tr!("Loading…"));
            state.controls.set_visible(true);
            (state.result.clone(), state.filter.clone())
        };
        let weak = Rc::downgrade(&self.0);
        gtk4::glib::spawn_future_local(async move {
            let worker_cancel = cancel.clone();
            let worker_keys = keys.clone();
            let planned = gtk4::gio::spawn_blocking(move || {
                let mut indices = Vec::with_capacity(result.rows.len());
                for (index, row) in result.rows.iter().enumerate() {
                    if worker_cancel.load(Ordering::Relaxed) {
                        return None;
                    }
                    let keep = filter.as_ref().is_none_or(|filter| {
                        row.get(filter.column)
                            .is_some_and(|candidate| same_filter_value(candidate, &filter.value) != filter.exclude)
                    });
                    if keep {
                        indices.push(index);
                    }
                }
                let order =
                    local_sort::sorted_source_indices_cancellable(&result, &worker_keys, indices, &worker_cancel)?;
                PreparedSourceRows::checked(result, order, &worker_cancel)
            })
            .await;
            let Some(inner) = weak.upgrade() else { return };
            let view = Self(inner);
            {
                let mut state = view.0.borrow_mut();
                if state
                    .pending
                    .as_ref()
                    .is_none_or(|current| !Arc::ptr_eq(current, &cancel))
                {
                    return;
                }
                state.pending = None;
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
            }
            if let Ok(Some(order)) = planned
                && view.apply_prepared_order(&order)
            {
                view.0.borrow_mut().sort = keys;
                view.persist_settings();
            }
            update_status(&view.0.borrow());
        });
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
        self.apply_order_with_selection(order, store, selection, selected);
    }

    fn apply_prepared_order(&self, order: &PreparedSourceRows) -> bool {
        let (store, selection, source_row_count) = {
            let state = self.0.borrow();
            let Some(store) = state.store.clone() else { return false };
            let Some(selection) = state.selection.clone() else {
                return false;
            };
            (store, selection, state.result.rows.len())
        };
        let item_count = store.n_items();
        let all_selected = item_count > 0
            && item_count as usize == source_row_count
            && selection.selection().size() == u64::from(item_count);
        let previous_selection = (!all_selected).then(|| selection.selection().copy());
        let selected = (!all_selected).then(|| selected_source_rows(&store, &selection));
        let syncing = self.0.borrow().selection_syncing.clone();
        syncing.set(true);
        if all_selected {
            selection.set_model(None::<&RowStore>);
        } else if previous_selection.as_ref().is_some_and(|selected| selected.size() > 0) {
            selection.set_selection(&gtk4::Bitset::new_empty(), &gtk4::Bitset::new_range(0, item_count));
        }
        let applied = store.replace_prepared_source_rows(order);
        if applied {
            if all_selected {
                selection.set_model(Some(&store));
                selection.select_all();
            } else if let Some(selected) = selected.as_ref() {
                apply_selected_order(order.indices(), &selection, selected);
            }
        } else if all_selected {
            selection.set_model(Some(&store));
            selection.select_all();
        } else if let Some(previous_selection) = previous_selection.filter(|selected| selected.size() > 0) {
            selection.set_selection(&previous_selection, &gtk4::Bitset::new_range(0, item_count));
        }
        syncing.set(false);
        applied
    }

    fn apply_order_with_selection(
        &self,
        order: Vec<usize>,
        store: RowStore,
        selection: gtk4::MultiSelection,
        selected: HashSet<usize>,
    ) {
        let syncing = self.0.borrow().selection_syncing.clone();
        syncing.set(true);
        if !store.replace_source_rows(&order) {
            syncing.set(false);
            return;
        }
        apply_selected_order(&order, &selection, &selected);
        syncing.set(false);
        update_status(&self.0.borrow());
    }
}

fn apply_selected_order(order: &[usize], selection: &gtk4::MultiSelection, selected: &HashSet<usize>) {
    let positions = gtk4::Bitset::new_empty();
    if !selected.is_empty() {
        for (position, source_index) in order.iter().enumerate() {
            if selected.contains(source_index) {
                positions.add(position as u32);
            }
        }
    }
    selection.set_selection(&positions, &gtk4::Bitset::new_range(0, order.len() as u32));
}

fn restore_selection(
    store: &RowStore,
    selection: &gtk4::MultiSelection,
    selected: &SavedSelection,
    has_saved_view_order: bool,
    source_row_count: usize,
    selection_syncing: &Cell<bool>,
) {
    selection_syncing.set(true);
    let item_count = store.n_items();
    let (all_source_rows_selected, selected_count) = match selected {
        SavedSelection::All => (true, source_row_count),
        SavedSelection::Rows(rows) => (rows.len() == source_row_count, rows.len()),
    };
    if all_source_rows_selected || (!has_saved_view_order && selected_count == item_count as usize) {
        selection.select_all();
    } else {
        let positions = gtk4::Bitset::new_empty();
        if let SavedSelection::Rows(rows) = selected {
            if !has_saved_view_order && rows.len().saturating_mul(4) < item_count as usize {
                for source in rows.iter() {
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
                        .is_some_and(|source| rows.contains(&source))
                    {
                        positions.add(position);
                    }
                }
            }
        }
        selection.set_selection(&positions, &gtk4::Bitset::new_range(0, item_count));
    }
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
        (true, false) => bookie_app_services::tr!("Clear filter"),
        (false, true) => bookie_app_services::tr!("Clear sort"),
        _ => bookie_app_services::tr!("Clear"),
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
    Some(bookie_app_services::tr!("Sorted by {keys}").replace("{keys}", &parts.join(", ")))
}

fn filter_summary(visible: u128, loaded: u128, truncated: bool) -> String {
    let mut summary = bookie_app_services::tr!("Showing {visible} of {loaded} loaded rows")
        .replace("{visible}", &crate::ui::count_format::format_thousands(visible))
        .replace("{loaded}", &crate::ui::count_format::format_thousands(loaded));
    if truncated {
        summary.push_str(" · ");
        summary.push_str(&bookie_app_services::tr!("result truncated"));
    }
    summary
}

fn truncation_notice(sorted: bool, loaded_rows: usize) -> String {
    if sorted {
        return bookie_app_services::tr!(
            "Sorted the loaded rows only; the result is truncated. Use ORDER BY for the full result."
        );
    }
    bookie_app_services::tr!(
        "Only the first {loaded} rows were loaded; the result is truncated. Narrow the query to see the rest."
    )
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
