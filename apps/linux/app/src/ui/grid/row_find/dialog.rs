use std::cell::{Cell, RefCell};
use std::rc::Rc;

use bookie_app_services::text_match::TextMatchOptions;
use bookie_core::Value;
use gtk4 as gtk;
use gtk4::glib;
use libadwaita::{self as adw, prelude::*};
use tokio_util::sync::CancellationToken;

use super::super::super::row_object::RowObject;
use super::super::GridMsg;
use super::query::{FindQuery, MAX_MATCHES, matching_rows};

const PREVIEW_CHARS: usize = 80;

pub(crate) struct FindTarget {
    pub names: Vec<String>,
    pub editable: Vec<bool>,
    pub pk_indices: Vec<usize>,
    pub generation: u64,
    pub edit: Option<relm4::Sender<GridMsg>>,
}

#[derive(Default)]
struct ReplaceOperation {
    generation: Cell<u64>,
    active: RefCell<Option<(u64, CancellationToken)>>,
}

struct ReplaceCapture {
    view: glib::WeakRef<gtk::ColumnView>,
    model: gtk::SelectionModel,
    row_store: crate::ui::row_store::RowStore,
    row_store_generation: u64,
    total: u32,
    next_position: Cell<u32>,
    only_position: Option<u32>,
    snapshot: RefCell<bookie_app_services::grid_replace::ReplaceSnapshot>,
    query: FindQuery,
    replacement: String,
    editable: Vec<bool>,
    pk_indices: Vec<usize>,
    cancel: CancellationToken,
    sender: relm4::Sender<GridMsg>,
    widgets: FindWidgets,
    operation: Rc<ReplaceOperation>,
    id: u64,
}

struct ReplacePlan {
    snapshot: bookie_app_services::grid_replace::ReplaceSnapshot,
    query: FindQuery,
    replacement: String,
    editable: Vec<bool>,
    cancel: CancellationToken,
    sender: relm4::Sender<GridMsg>,
    widgets: FindWidgets,
    operation: Rc<ReplaceOperation>,
    id: u64,
    row_store_generation: u64,
}

#[derive(Clone)]
struct FindWidgets {
    content: gtk::Box,
    search: gtk::SearchEntry,
    replace_entry: gtk::Entry,
    match_case: gtk::ToggleButton,
    whole_words: gtk::ToggleButton,
    regex: gtk::ToggleButton,
    columns: gtk::DropDown,
    status: gtk::Label,
    list: gtk::ListBox,
    replace_one: gtk::Button,
    replace_all: gtk::Button,
    cancel_replace: gtk::Button,
}

fn flat_toggle(label: &str, tooltip: &str) -> gtk::ToggleButton {
    let toggle = gtk::ToggleButton::builder().label(label).tooltip_text(tooltip).build();
    toggle.add_css_class("flat");
    toggle
}

fn build_widgets(names: &[String], can_replace: bool) -> FindWidgets {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.append(&adw::HeaderBar::new());
    let search = gtk::SearchEntry::builder()
        .placeholder_text(crate::tr!("Search the loaded rows"))
        .margin_start(12)
        .margin_end(12)
        .hexpand(true)
        .build();
    search.update_property(&[gtk::accessible::Property::Label(&crate::tr!("Search the loaded rows"))]);
    let match_case = flat_toggle("Aa", &crate::tr!("Match case"));
    let whole_words = flat_toggle("W", &crate::tr!("Whole words"));
    let regex = flat_toggle(".*", &crate::tr!("Regular expression"));
    let columns = column_dropdown(names);
    let find_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    find_row.set_margin_start(12);
    find_row.set_margin_end(12);
    find_row.append(&search);
    find_row.append(&match_case);
    find_row.append(&whole_words);
    find_row.append(&regex);
    find_row.append(&columns);
    content.append(&find_row);
    let (replace_row, replace_entry, replace_one, replace_all, cancel_replace) = replace_controls(can_replace);
    content.append(&replace_row);
    let status = gtk::Label::builder()
        .xalign(0.0)
        .margin_start(12)
        .margin_end(12)
        .build();
    status.add_css_class("dim-label");
    content.append(&status);
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(true)
        .build();
    list.add_css_class("boxed-list");
    content.append(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(12)
            .build(),
    );
    FindWidgets {
        content,
        search,
        replace_entry,
        match_case,
        whole_words,
        regex,
        columns,
        status,
        list,
        replace_one,
        replace_all,
        cancel_replace,
    }
}

fn column_dropdown(names: &[String]) -> gtk::DropDown {
    let mut labels = vec![crate::tr!("All columns")];
    labels.extend(names.iter().cloned());
    let model = gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>());
    let dropdown = gtk::DropDown::new(Some(model), gtk::Expression::NONE);
    dropdown.set_tooltip_text(Some(&crate::tr!("Column scope")));
    dropdown
}

fn replace_controls(visible: bool) -> (gtk::Box, gtk::Entry, gtk::Button, gtk::Button, gtk::Button) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_visible(visible);
    let entry = gtk::Entry::builder()
        .placeholder_text(crate::tr!("Replace"))
        .hexpand(true)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label(&crate::tr!("Replace"))]);
    let replace_one = gtk::Button::with_label(&crate::tr!("Replace"));
    replace_one.add_css_class("flat");
    let replace_all = gtk::Button::with_label(&crate::tr!("Replace all"));
    replace_all.add_css_class("flat");
    let cancel_replace = gtk::Button::with_label(&crate::tr!("Cancel"));
    cancel_replace.add_css_class("flat");
    cancel_replace.set_visible(false);
    row.append(&entry);
    row.append(&replace_one);
    row.append(&replace_all);
    row.append(&cancel_replace);
    (row, entry, replace_one, replace_all, cancel_replace)
}

fn result_row(label: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(
        &gtk::Label::builder()
            .label(label)
            .xalign(0.0)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build(),
    ));
    row.update_property(&[gtk::accessible::Property::Label(label)]);
    row
}

pub(super) fn preview(text: &str) -> String {
    let single_line: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    if single_line.chars().count() <= PREVIEW_CHARS {
        return single_line;
    }
    let cut: String = single_line.chars().take(PREVIEW_CHARS).collect();
    format!("{cut}…")
}

fn status_text(query: &str, shown: usize, problem: Option<&str>) -> String {
    if let Some(problem) = problem {
        return problem.to_owned();
    }
    if query.trim().is_empty() {
        String::new()
    } else if shown == 0 {
        crate::tr!("No matching rows")
    } else if shown >= MAX_MATCHES {
        crate::tr!("The first {count} matches").replace("{count}", &MAX_MATCHES.to_string())
    } else {
        crate::tr!("{count} matching rows").replace("{count}", &shown.to_string())
    }
}

fn current_query(widgets: &FindWidgets) -> FindQuery {
    let selected = widgets.columns.selected();
    let column = (selected > 0).then(|| selected as usize - 1);
    FindQuery::new(
        widgets.search.text().to_string(),
        TextMatchOptions {
            match_case: widgets.match_case.is_active(),
            whole_words: widgets.whole_words.is_active(),
            regex: widgets.regex.is_active(),
        },
        column,
    )
}

fn show_matches(widgets: &FindWidgets, model: &gtk::SelectionModel, names: &[String]) -> Vec<(u32, usize)> {
    let query = current_query(widgets);
    let row_store = model
        .downcast_ref::<gtk::MultiSelection>()
        .and_then(|selection| selection.model())
        .and_then(|model| model.downcast::<crate::ui::row_store::RowStore>().ok());
    let found = matching_rows(
        model.n_items(),
        |position, visit| {
            if let Some(store) = row_store.as_ref() {
                store.visit_search_cells(position, visit);
            } else if let Some(row) = model.item(position).and_then(|item| item.downcast::<RowObject>().ok()) {
                row.visit_cells(visit);
            }
        },
        &query,
        MAX_MATCHES,
    );
    while let Some(row) = widgets.list.row_at_index(0) {
        widgets.list.remove(&row);
    }
    let (hits, problem) = match found {
        Ok(hits) => (hits, None),
        Err(_) => (Vec::new(), Some(crate::tr!("Invalid regular expression"))),
    };
    for hit in &hits {
        let column = names.get(hit.column).map_or("", String::as_str);
        let label = format!("{} · {}: {}", hit.position + 1, column, preview(&hit.text));
        widgets.list.append(&result_row(&label));
    }
    widgets.list.select_row(widgets.list.row_at_index(0).as_ref());
    widgets
        .status
        .set_text(&status_text(&query.needle, hits.len(), problem.as_deref()));
    hits.into_iter().map(|hit| (hit.position, hit.column)).collect()
}

fn close_on(dialog: &adw::Dialog) -> impl Fn() + 'static {
    let dialog = dialog.downgrade();
    move || {
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    }
}

pub(super) fn present(view: &gtk::ColumnView, target: Rc<FindTarget>) {
    let dialog = adw::Dialog::builder()
        .title(crate::tr!("Find in Loaded Rows"))
        .content_width(560)
        .content_height(500)
        .build();
    let can_replace =
        target.edit.is_some() && !target.pk_indices.is_empty() && target.editable.iter().any(|editable| *editable);
    let widgets = Rc::new(build_widgets(&target.names, can_replace));
    let positions: Rc<RefCell<Vec<(u32, usize)>>> = Rc::default();
    let operation = Rc::new(ReplaceOperation::default());
    let refresh = refresh_fn(view, &widgets, &positions, &target.names);
    wire_search_controls(&widgets, &dialog, refresh.clone(), operation.clone());
    connect_replace(&widgets, view, target.clone(), positions.clone(), operation.clone());
    dialog.connect_closed(move |_| {
        cancel_replace_operation(&operation);
    });
    wire_result_selection(&dialog, view, &widgets, positions);
    dialog.set_child(Some(&widgets.content));
    dialog.present(Some(view));
    widgets.search.grab_focus();
}

fn refresh_fn(
    view: &gtk::ColumnView,
    widgets: &Rc<FindWidgets>,
    positions: &Rc<RefCell<Vec<(u32, usize)>>>,
    names: &[String],
) -> impl Fn() + Clone + 'static {
    let (view, widgets, positions, names) = (view.downgrade(), widgets.clone(), positions.clone(), names.to_vec());
    move || {
        if let Some(model) = view.upgrade().and_then(|view| view.model()) {
            *positions.borrow_mut() = show_matches(&widgets, &model, &names);
        }
    }
}

fn wire_search_controls(
    widgets: &FindWidgets,
    dialog: &adw::Dialog,
    refresh: impl Fn() + Clone + 'static,
    operation: Rc<ReplaceOperation>,
) {
    let invalidate = {
        let (operation, widgets) = (operation, widgets.clone());
        move || {
            if cancel_replace_operation(&operation) {
                widgets.status.set_text(&crate::tr!("Replacement cancelled"));
            }
            set_replace_running(&widgets, false);
        }
    };
    widgets.search.connect_search_changed({
        let (refresh, invalidate) = (refresh.clone(), invalidate.clone());
        move |_| {
            invalidate();
            refresh();
        }
    });
    for toggle in [&widgets.match_case, &widgets.whole_words, &widgets.regex] {
        toggle.connect_toggled({
            let (refresh, invalidate) = (refresh.clone(), invalidate.clone());
            move |_| {
                invalidate();
                refresh();
            }
        });
    }
    widgets.columns.connect_selected_notify({
        let (refresh, invalidate) = (refresh.clone(), invalidate.clone());
        move |_| {
            invalidate();
            refresh();
        }
    });
    widgets.replace_entry.connect_changed(move |_| invalidate());
    let close = close_on(dialog);
    widgets.search.connect_stop_search(move |_| close());
    widgets.search.connect_activate({
        let list = widgets.list.clone();
        move |_| {
            if let Some(row) = list.selected_row() {
                row.activate();
            }
        }
    });
}

fn wire_result_selection(
    dialog: &adw::Dialog,
    view: &gtk::ColumnView,
    widgets: &FindWidgets,
    positions: Rc<RefCell<Vec<(u32, usize)>>>,
) {
    let close = close_on(dialog);
    let selected_position = Rc::new(Cell::new(None));
    super::super::dialog_focus::restore_on_close(dialog, view, {
        let selected_position = selected_position.clone();
        move |view| {
            if let Some(position) = selected_position.get() {
                select_and_scroll(view, position);
            }
        }
    });
    widgets.list.connect_row_activated(move |_, row| {
        let position = usize::try_from(row.index())
            .ok()
            .and_then(|index| positions.borrow().get(index).map(|hit| hit.0));
        selected_position.set(position);
        close();
    });
}

fn connect_replace(
    widgets: &Rc<FindWidgets>,
    view: &gtk::ColumnView,
    target: Rc<FindTarget>,
    positions: Rc<RefCell<Vec<(u32, usize)>>>,
    operation: Rc<ReplaceOperation>,
) {
    if target.edit.is_none() {
        return;
    }
    widgets.replace_one.connect_clicked({
        let (view, widgets, target, positions, operation) = (
            view.downgrade(),
            widgets.clone(),
            target.clone(),
            positions,
            operation.clone(),
        );
        move |_| {
            let Some(view) = view.upgrade() else { return };
            replace_selected(&view, &widgets, &target, &positions.borrow(), &operation);
        }
    });
    widgets.replace_all.connect_clicked({
        let (view, widgets, target, operation) = (view.downgrade(), widgets.clone(), target, operation.clone());
        move |_| {
            let Some(view) = view.upgrade() else { return };
            replace_every(&view, &widgets, &target, &operation);
        }
    });
    widgets.cancel_replace.connect_clicked({
        let (operation, widgets) = (operation.clone(), widgets.clone());
        move |_| {
            cancel_replace_operation(&operation);
            set_replace_running(&widgets, false);
            widgets.status.set_text(&crate::tr!("Replacement cancelled"));
        }
    });
    let operation = operation.clone();
    widgets.replace_entry.connect_activate({
        let replace_all = widgets.replace_all.clone();
        move |_| {
            if operation.active.borrow().is_none() {
                replace_all.emit_clicked();
            }
        }
    });
}

fn replace_selected(
    view: &gtk::ColumnView,
    widgets: &FindWidgets,
    target: &FindTarget,
    positions: &[(u32, usize)],
    operation: &Rc<ReplaceOperation>,
) {
    let Some(index) = widgets.list.selected_row().map(|row| row.index() as usize) else {
        return;
    };
    let Some(&(position, column)) = positions.get(index) else {
        return;
    };
    start_replace(view, widgets, target, Some((position, column)), operation);
}

fn replace_every(view: &gtk::ColumnView, widgets: &FindWidgets, target: &FindTarget, operation: &Rc<ReplaceOperation>) {
    start_replace(view, widgets, target, None, operation);
}

fn start_replace(
    view: &gtk::ColumnView,
    widgets: &FindWidgets,
    target: &FindTarget,
    only: Option<(u32, usize)>,
    operation: &Rc<ReplaceOperation>,
) {
    let Some(model) = view.model() else {
        return;
    };
    let query = {
        let mut query = current_query(widgets);
        if let Some((_, column)) = only {
            query.column = Some(column);
        }
        query
    };
    let replacement = widgets.replace_entry.text().to_string();
    let row_store = model
        .downcast_ref::<gtk::MultiSelection>()
        .and_then(|selection| selection.model())
        .and_then(|model| model.downcast::<crate::ui::row_store::RowStore>().ok());
    let Some(row_store) = row_store else {
        widgets
            .status
            .set_text(&crate::tr!("These rows are no longer available"));
        return;
    };
    if row_store.generation() != target.generation {
        widgets
            .status
            .set_text(&crate::tr!("These rows changed; run the search again"));
        return;
    }
    let Some(sender) = target.edit.clone() else { return };
    let id = begin_replace_operation(operation);
    let cancel = operation
        .active
        .borrow()
        .as_ref()
        .map(|(_, cancel)| cancel.clone())
        .unwrap_or_default();
    set_replace_running(widgets, true);
    widgets.status.set_text(&crate::tr!("Preparing replacement…"));
    let total = only.map_or(model.n_items(), |(position, _)| position.saturating_add(1));
    let start_position = only.map_or(0, |(position, _)| position);
    let capture = ReplaceCapture {
        view: view.downgrade(),
        model,
        row_store_generation: row_store.generation(),
        row_store,
        total,
        next_position: Cell::new(start_position),
        only_position: only.map(|(position, _)| position),
        snapshot: RefCell::new(bookie_app_services::grid_replace::ReplaceSnapshot::default()),
        query,
        replacement,
        editable: target.editable.clone(),
        pk_indices: target.pk_indices.clone(),
        cancel,
        sender,
        widgets: widgets.clone(),
        operation: Rc::clone(operation),
        id,
    };
    glib::idle_add_local(move || collect_snapshot_chunk(&capture));
}

fn collect_snapshot_chunk(capture: &ReplaceCapture) -> glib::ControlFlow {
    if !replace_operation_is_current(&capture.operation, capture.id, &capture.cancel) {
        return glib::ControlFlow::Break;
    }
    let Some(view) = capture.view.upgrade() else {
        cancel_replace_operation(&capture.operation);
        return glib::ControlFlow::Break;
    };
    if view.model().is_none_or(|current| current != capture.model)
        || capture.row_store.generation() != capture.row_store_generation
    {
        finish_replace_ui(
            &capture.widgets,
            &capture.operation,
            capture.id,
            crate::tr!("These rows changed; run the search again"),
        );
        return glib::ControlFlow::Break;
    }
    let start = capture.next_position.get();
    let end = start.saturating_add(64).min(capture.total);
    if let Err(error) = capture_snapshot_rows(capture, start, end) {
        finish_replace_ui(
            &capture.widgets,
            &capture.operation,
            capture.id,
            replace_error_text(error),
        );
        return glib::ControlFlow::Break;
    }
    capture.next_position.set(end);
    if end < capture.total {
        return glib::ControlFlow::Continue;
    }
    let snapshot = std::mem::take(&mut *capture.snapshot.borrow_mut());
    plan_and_apply_replacements(ReplacePlan {
        snapshot,
        query: capture.query.clone(),
        replacement: capture.replacement.clone(),
        editable: capture.editable.clone(),
        cancel: capture.cancel.clone(),
        sender: capture.sender.clone(),
        widgets: capture.widgets.clone(),
        operation: Rc::clone(&capture.operation),
        id: capture.id,
        row_store_generation: capture.row_store_generation,
    });
    glib::ControlFlow::Break
}

fn capture_snapshot_rows(capture: &ReplaceCapture, start: u32, end: u32) -> Result<(), super::replace::ReplaceError> {
    for position in start..end {
        if capture.only_position.is_some_and(|only| only != position) {
            continue;
        }
        let key =
            row_key_values(position, &capture.model, Some(&capture.row_store), &capture.pk_indices).unwrap_or_default();
        capture
            .snapshot
            .borrow_mut()
            .push_row_cancellable(position, &key, Some(&capture.cancel), |visit| {
                capture.row_store.visit_search_cells(position, |column, value| {
                    if capture.query.column.is_none_or(|wanted| wanted == column) {
                        visit(column, value);
                    }
                    false
                });
            })?;
    }
    Ok(())
}

fn plan_and_apply_replacements(plan: ReplacePlan) {
    let ReplacePlan {
        snapshot,
        query,
        replacement,
        editable,
        cancel,
        sender,
        widgets,
        operation,
        id,
        row_store_generation,
    } = plan;
    glib::spawn_future_local(async move {
        let worker_cancel = cancel.clone();
        let planned = gtk::gio::spawn_blocking(move || {
            super::replace::plan_snapshot_replacements(&snapshot, &query, &replacement, &editable, &worker_cancel)
        })
        .await;
        let report = match planned {
            Ok(Ok(report)) => report,
            Ok(Err(error)) => {
                finish_replace_ui(&widgets, &operation, id, replace_error_text(error));
                return;
            }
            Err(_) => {
                finish_replace_ui(
                    &widgets,
                    &operation,
                    id,
                    crate::tr!("Replacement could not be completed"),
                );
                return;
            }
        };
        if !replace_operation_is_current(&operation, id, &cancel) {
            return;
        }
        let (reply, result) = tokio::sync::oneshot::channel();
        if sender
            .send(GridMsg::ReplaceBatch {
                report,
                generation: row_store_generation,
                cancel: cancel.clone(),
                reply,
            })
            .is_err()
        {
            finish_replace_ui(&widgets, &operation, id, crate::tr!("Replacement could not be applied"));
            return;
        }
        finish_replace_apply(&widgets, &operation, id, result.await);
    });
}

fn finish_replace_apply(
    widgets: &FindWidgets,
    operation: &ReplaceOperation,
    id: u64,
    result: Result<
        Result<
            bookie_app_services::grid_replace::ReplaceAcknowledgement,
            bookie_app_services::grid_replace::ReplaceApplyError,
        >,
        tokio::sync::oneshot::error::RecvError,
    >,
) {
    match result {
        Ok(Ok(ack)) => {
            while let Some(row) = widgets.list.row_at_index(0) {
                widgets.list.remove(&row);
            }
            finish_replace_ui(widgets, operation, id, replace_status(ack.replacements, ack.skipped));
        }
        Ok(Err(error)) => finish_replace_ui(widgets, operation, id, replace_apply_error_text(error)),
        Err(_) => finish_replace_ui(widgets, operation, id, crate::tr!("Replacement could not be applied")),
    }
}

fn begin_replace_operation(operation: &ReplaceOperation) -> u64 {
    cancel_replace_operation(operation);
    let id = operation.generation.get().wrapping_add(1);
    operation.generation.set(id);
    operation.active.replace(Some((id, CancellationToken::new())));
    id
}

fn cancel_replace_operation(operation: &ReplaceOperation) -> bool {
    operation.generation.set(operation.generation.get().wrapping_add(1));
    if let Some((_, cancel)) = operation.active.borrow_mut().take() {
        cancel.cancel();
        true
    } else {
        false
    }
}

fn replace_operation_is_current(operation: &ReplaceOperation, id: u64, cancel: &CancellationToken) -> bool {
    operation.generation.get() == id && !cancel.is_cancelled()
}

fn set_replace_running(widgets: &FindWidgets, running: bool) {
    widgets.replace_one.set_sensitive(!running);
    widgets.replace_all.set_sensitive(!running);
    widgets.cancel_replace.set_visible(running);
}

fn finish_replace_ui(widgets: &FindWidgets, operation: &ReplaceOperation, id: u64, message: String) {
    if operation.generation.get() != id {
        return;
    }
    if operation
        .active
        .borrow()
        .as_ref()
        .is_some_and(|(active, _)| *active == id)
    {
        operation.active.borrow_mut().take();
    }
    set_replace_running(widgets, false);
    widgets.status.set_text(&message);
}

fn replace_error_text(error: super::replace::ReplaceError) -> String {
    match error {
        super::replace::ReplaceError::Cancelled => crate::tr!("Replacement cancelled"),
        super::replace::ReplaceError::Budget => {
            crate::tr!("Replacement is too large; narrow the selection or use a shorter replacement")
        }
        super::replace::ReplaceError::Match(_) => crate::tr!("Invalid regular expression"),
        super::replace::ReplaceError::Stale => crate::tr!("These rows changed; run the search again"),
    }
}

fn replace_apply_error_text(error: bookie_app_services::grid_replace::ReplaceApplyError) -> String {
    use bookie_app_services::grid_replace::ReplaceApplyError;
    match error {
        ReplaceApplyError::Cancelled => crate::tr!("Replacement cancelled"),
        ReplaceApplyError::Stale => crate::tr!("These rows changed; run the search again"),
        ReplaceApplyError::NotEditable => crate::tr!("One or more cells can no longer be edited"),
        ReplaceApplyError::Saving => crate::tr!("Finish saving changes before replacing values"),
        ReplaceApplyError::InvalidValue => crate::tr!("One or more replacement values are invalid"),
    }
}

fn replace_status(replacements: usize, skipped: usize) -> String {
    if replacements == 0 && skipped == 0 {
        crate::tr!("No replacements")
    } else if skipped == 0 {
        crate::tr!("Replaced {count} matches").replace("{count}", &replacements.to_string())
    } else {
        crate::tr!("Replaced {count} matches · skipped {skipped} cells")
            .replace("{count}", &replacements.to_string())
            .replace("{skipped}", &skipped.to_string())
    }
}

fn row_key_values(
    position: u32,
    model: &gtk::SelectionModel,
    store: Option<&crate::ui::row_store::RowStore>,
    pk_indices: &[usize],
) -> Option<Vec<Value>> {
    if pk_indices.is_empty() {
        return None;
    }
    let mut values = vec![None; pk_indices.len()];
    if let Some(store) = store {
        store.visit_search_cells(position, |column, value| {
            if let Some(slot) = pk_indices.iter().position(|&wanted| wanted == column) {
                values[slot] = Some(value.clone());
            }
            false
        });
    } else {
        let row = model
            .item(position)
            .and_then(|item| item.downcast::<RowObject>().ok())?;
        for (slot, &column) in pk_indices.iter().enumerate() {
            values[slot] = Some(row.cell_value(column));
        }
    }
    values.into_iter().collect()
}

fn select_and_scroll(view: &gtk::ColumnView, position: u32) {
    let Some(model) = view.model().filter(|model| position < model.n_items()) else {
        return;
    };
    model.select_item(position, true);
    view.scroll_to(
        position,
        None::<&gtk::ColumnViewColumn>,
        gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
        None,
    );
}
