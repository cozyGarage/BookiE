use std::cell::{Cell, RefCell};
use std::rc::Rc;

use bookie_app_services::text_match::TextMatchOptions;
use bookie_core::Value;
use gtk4 as gtk;
use libadwaita::{self as adw, prelude::*};
use tokio_util::sync::CancellationToken;

use super::super::super::row_object::RowObject;
use super::super::GridMsg;
use super::query::{FindQuery, MAX_MATCHES, matching_rows};
use super::replace::plan_replacements;

const PREVIEW_CHARS: usize = 80;

pub(crate) struct FindTarget {
    pub names: Vec<String>,
    pub editable: Vec<bool>,
    pub pk_indices: Vec<usize>,
    pub edit: Option<relm4::Sender<GridMsg>>,
}

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
    let (replace_row, replace_entry, replace_one, replace_all) = replace_controls(can_replace);
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

fn replace_controls(visible: bool) -> (gtk::Box, gtk::Entry, gtk::Button, gtk::Button) {
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
    row.append(&entry);
    row.append(&replace_one);
    row.append(&replace_all);
    (row, entry, replace_one, replace_all)
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
    let refresh = refresh_fn(view, &widgets, &positions, &target.names);
    wire_search_controls(&widgets, &dialog, refresh.clone());
    connect_replace(&widgets, view, target.clone(), positions.clone(), refresh);
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

fn wire_search_controls(widgets: &FindWidgets, dialog: &adw::Dialog, refresh: impl Fn() + Clone + 'static) {
    widgets.search.connect_search_changed({
        let refresh = refresh.clone();
        move |_| refresh()
    });
    for toggle in [&widgets.match_case, &widgets.whole_words, &widgets.regex] {
        toggle.connect_toggled({
            let refresh = refresh.clone();
            move |_| refresh()
        });
    }
    widgets.columns.connect_selected_notify({
        let refresh = refresh.clone();
        move |_| refresh()
    });
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
    refresh: impl Fn() + Clone + 'static,
) {
    if target.edit.is_none() {
        return;
    }
    widgets.replace_one.connect_clicked({
        let (view, widgets, target, positions, refresh) = (
            view.downgrade(),
            widgets.clone(),
            target.clone(),
            positions,
            refresh.clone(),
        );
        move |_| {
            let Some(view) = view.upgrade() else { return };
            replace_selected(&view, &widgets, &target, &positions.borrow(), &refresh);
        }
    });
    widgets.replace_all.connect_clicked({
        let (view, widgets, target, refresh) = (view.downgrade(), widgets.clone(), target, refresh);
        move |_| {
            let Some(view) = view.upgrade() else { return };
            replace_every(&view, &widgets, &target, &refresh);
        }
    });
}

fn replace_selected(
    view: &gtk::ColumnView,
    widgets: &FindWidgets,
    target: &FindTarget,
    positions: &[(u32, usize)],
    refresh: &impl Fn(),
) {
    let Some(index) = widgets.list.selected_row().map(|row| row.index() as usize) else {
        return;
    };
    let Some(&(position, column)) = positions.get(index) else {
        return;
    };
    let report = run_replace(view, widgets, target, Some((position, column)));
    finish_replace(widgets, target, report, refresh);
}

fn replace_every(view: &gtk::ColumnView, widgets: &FindWidgets, target: &FindTarget, refresh: &impl Fn()) {
    let report = run_replace(view, widgets, target, None);
    finish_replace(widgets, target, report, refresh);
}

fn run_replace(
    view: &gtk::ColumnView,
    widgets: &FindWidgets,
    target: &FindTarget,
    only: Option<(u32, usize)>,
) -> Result<super::replace::ReplaceReport, bookie_app_services::text_match::TextMatchError> {
    let Some(model) = view.model() else {
        return Ok(super::replace::ReplaceReport {
            edits: Vec::new(),
            replacements: 0,
            skipped: 0,
        });
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
    let cancel = CancellationToken::new();
    let only_position = only.map(|(position, _)| position);
    plan_replacements(
        model.n_items(),
        |position, visit| {
            if only_position.is_some_and(|wanted| wanted != position) {
                return;
            }
            if let Some(store) = row_store.as_ref() {
                store.visit_search_cells(position, |column, value| {
                    visit(column, value);
                    false
                });
            } else if let Some(row) = model.item(position).and_then(|item| item.downcast::<RowObject>().ok()) {
                row.visit_cells(|column, value| {
                    visit(column, value);
                    false
                });
            }
        },
        |position| row_key_values(position, &model, row_store.as_ref(), &target.pk_indices),
        &query,
        &replacement,
        &target.editable,
        &cancel,
    )
}

fn finish_replace(
    widgets: &FindWidgets,
    target: &FindTarget,
    report: Result<super::replace::ReplaceReport, bookie_app_services::text_match::TextMatchError>,
    refresh: &impl Fn(),
) {
    let Ok(report) = report else {
        widgets.status.set_text(&crate::tr!("Invalid regular expression"));
        return;
    };
    let Some(sender) = &target.edit else {
        return;
    };
    for edit in &report.edits {
        sender
            .send(GridMsg::CellEdited {
                row_position: edit.position,
                col_index: edit.column,
                new_value: edit.new_value.clone(),
                row_key: edit.row_key.clone(),
            })
            .ok();
    }
    widgets.status.set_text(&replace_status(&report));
    refresh();
}

fn replace_status(report: &super::replace::ReplaceReport) -> String {
    if report.edits.is_empty() && report.skipped == 0 {
        crate::tr!("No replacements")
    } else if report.skipped == 0 {
        crate::tr!("Replaced {count} matches").replace("{count}", &report.replacements.to_string())
    } else {
        crate::tr!("Replaced {count} matches · skipped {skipped} cells")
            .replace("{count}", &report.replacements.to_string())
            .replace("{skipped}", &report.skipped.to_string())
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
    } else if let Some(row) = model.item(position).and_then(|item| item.downcast::<RowObject>().ok()) {
        for (slot, &column) in pk_indices.iter().enumerate() {
            values[slot] = Some(row.cell_value(column));
        }
    } else {
        return None;
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
