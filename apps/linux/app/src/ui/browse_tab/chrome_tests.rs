use bookie_core::Value;
use relm4::adw::prelude::{ActionRowExt, PreferencesRowExt};
use relm4::gtk::prelude::*;
use relm4::{ComponentController, Controller, gtk};
use uuid::Uuid;

use super::save_in_flight_tests::{column, launch_with_read_only, load_two_rows, pump, transaction_save_id};
use super::{BrowseTab, BrowseTabInput, BrowseTabOutput};

fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut child = root.first_child();
    while let Some(widget) = child {
        out.extend(descendants(&widget));
        child = widget.next_sibling();
        out.push(widget);
    }
    out
}

fn launch_tab(read_only: bool) -> (Controller<BrowseTab>, std::sync::mpsc::Receiver<BrowseTabOutput>) {
    relm4::gtk::init().unwrap();
    launch_with_read_only(Uuid::new_v4(), read_only)
}

fn inspector_value(controller: &Controller<BrowseTab>, column: &str) -> Option<String> {
    let model = controller.model();
    let widget: &gtk::Widget = model.inspector.widget().upcast_ref();
    descendants(widget)
        .into_iter()
        .filter_map(|widget| widget.downcast::<relm4::adw::ActionRow>().ok())
        .find(|row| row.title().as_str() == format!("{column} · text"))
        .and_then(|row| row.subtitle().map(|subtitle| subtitle.to_string()))
}

fn load_title_page(
    controller: &Controller<BrowseTab>,
    outputs: &std::sync::mpsc::Receiver<BrowseTabOutput>,
    title: &str,
) {
    let columns = vec![column("id", true), column("title", false), column("body", false)];
    let request = controller.model().begin_page_request();
    controller
        .sender()
        .send(BrowseTabInput::RowsLoaded {
            request,
            result: bookie_core::QueryResult {
                columns,
                rows: vec![vec![Value::Int(1), Value::Text(title.into()), Value::Text("b".into())]].into(),
                truncated: false,
            },
            projected_columns: None,
            elapsed: std::time::Duration::ZERO,
        })
        .unwrap();
    pump(outputs);
}

#[test]
#[ignore = "requires isolated GTK display"]
fn selected_row_details_refresh_after_save_and_discard() {
    let (controller, outputs) = launch_tab(false);
    load_two_rows(&controller, &outputs);
    controller
        .model()
        .current_selection
        .as_ref()
        .unwrap()
        .select_item(0, false);
    pump(&outputs);
    assert_eq!(inspector_value(&controller, "title").as_deref(), Some("a"));
    let selected_key = controller.model().row_key_at(0).expect("row primary key").0;
    assert_eq!(controller.model().find_row_position_by_key(&selected_key), Some(0));

    controller
        .sender()
        .send(BrowseTabInput::GridCellEdited {
            row_position: 0,
            col_index: 1,
            new_value: "saved".into(),
            value: None,
            row_key: vec![Value::Int(1)],
            draft_id: None,
        })
        .unwrap();
    pump(&outputs);
    assert!(controller.model().current_selection.as_ref().unwrap().is_selected(0));
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    let save = transaction_save_id(&pump(&outputs));
    controller.sender().send(BrowseTabInput::SaveCompleted(save)).unwrap();
    pump(&outputs);
    load_title_page(&controller, &outputs, "saved");
    assert_eq!(
        controller.model().current_result.as_ref().unwrap().rows[0][1],
        Value::Text("saved".into())
    );
    assert_eq!(controller.model().find_row_position_by_key(&selected_key), Some(0));
    assert_eq!(
        controller
            .model()
            .current_selection
            .as_ref()
            .unwrap()
            .selection()
            .size(),
        1,
        "the saved row should remain selected"
    );
    assert_eq!(inspector_value(&controller, "title").as_deref(), Some("saved"));

    controller
        .sender()
        .send(BrowseTabInput::GridCellEdited {
            row_position: 0,
            col_index: 1,
            new_value: "temporary".into(),
            value: None,
            row_key: vec![Value::Int(1)],
            draft_id: None,
        })
        .unwrap();
    pump(&outputs);
    controller.sender().send(BrowseTabInput::DiscardAll).unwrap();
    pump(&outputs);
    load_title_page(&controller, &outputs, "saved");
    assert_eq!(inspector_value(&controller, "title").as_deref(), Some("saved"));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn independent_browse_owners_keep_edits_and_teardown_separate() {
    relm4::gtk::init().unwrap();
    let id = Uuid::new_v4();
    let (first, first_outputs) = launch_with_read_only(id, false);
    let (second, second_outputs) = launch_with_read_only(id, false);
    load_two_rows(&first, &first_outputs);
    load_two_rows(&second, &second_outputs);
    first.sender().send(BrowseTabInput::InsertRow).unwrap();
    pump(&first_outputs);
    assert!(first.model().change_trackers.any_pending_in([id]));
    assert!(!second.model().change_trackers.any_pending_in([id]));
    second.sender().send(BrowseTabInput::InsertRow).unwrap();
    pump(&second_outputs);
    assert!(second.model().change_trackers.any_pending_in([id]));
    first.sender().send(BrowseTabInput::CommitSave).unwrap();
    let started = pump(&first_outputs);
    assert!(
        started
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::ExecuteTransaction { .. }))
    );
    first
        .sender()
        .send(BrowseTabInput::SaveCompleted(transaction_save_id(&started)))
        .unwrap();
    pump(&first_outputs);
    assert!(!first.model().change_trackers.any_pending_in([id]));
    assert!(second.model().change_trackers.any_pending_in([id]));
    first.model().change_trackers.close_tab(id);
    assert!(first.model().change_trackers.with_tab_ref(id, |_| ()).is_none());
    assert!(second.model().change_trackers.any_pending_in([id]));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn late_save_completion_does_not_discard_new_edits() {
    let (controller, outputs) = launch_tab(false);
    load_two_rows(&controller, &outputs);
    controller.sender().send(BrowseTabInput::InsertRow).unwrap();
    pump(&outputs);
    let id = controller.model().tab_id;
    assert!(controller.model().change_trackers.any_pending_in([id]));
    controller
        .sender()
        .send(BrowseTabInput::SaveCompleted(Uuid::new_v4()))
        .unwrap();
    pump(&outputs);
    assert!(controller.model().change_trackers.any_pending_in([id]));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn active_save_completion_clears_submitted_edits() {
    let (controller, outputs) = launch_tab(false);
    load_two_rows(&controller, &outputs);
    controller.sender().send(BrowseTabInput::InsertRow).unwrap();
    pump(&outputs);
    let id = controller.model().tab_id;
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    let started = pump(&outputs);
    assert!(
        started
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::ExecuteTransaction { .. }))
    );
    controller
        .sender()
        .send(BrowseTabInput::SaveCompleted(transaction_save_id(&started)))
        .unwrap();
    pump(&outputs);
    assert!(!controller.model().change_trackers.any_pending_in([id]));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn a_row_count_queued_before_refresh_is_not_shown() {
    let (controller, outputs) = launch_tab(false);
    controller
        .sender()
        .send(BrowseTabInput::ColumnsLoaded(vec![column("id", true)]))
        .unwrap();
    pump(&outputs);
    let stale = controller.model().row_count_requests.begin();
    controller.sender().send(BrowseTabInput::Refresh).unwrap();
    controller
        .sender()
        .send(BrowseTabInput::RowCountLoaded {
            request: stale,
            count: 7,
            is_estimated: false,
        })
        .unwrap();
    pump(&outputs);
    assert_eq!(controller.model().current_total_rows, None);
}

#[test]
#[ignore = "requires isolated GTK display"]
fn a_loaded_page_keeps_its_total_unknown_when_row_count_fails() {
    let (controller, outputs) = launch_tab(false);
    controller
        .sender()
        .send(BrowseTabInput::ColumnsLoaded(vec![column("id", true)]))
        .unwrap();
    pump(&outputs);
    let request = controller.model().page_requests.begin(0);
    controller
        .sender()
        .send(BrowseTabInput::RowsLoaded {
            request,
            result: bookie_core::QueryResult {
                columns: Vec::new(),
                rows: vec![vec![Value::Int(1)]].into(),
                truncated: false,
            },
            projected_columns: None,
            elapsed: std::time::Duration::ZERO,
        })
        .unwrap();
    pump(&outputs);
    let count_request = controller.model().row_count_requests.begin();
    controller
        .sender()
        .send(BrowseTabInput::RowCountFailed(count_request))
        .unwrap();
    pump(&outputs);

    assert_eq!(controller.model().current_total_rows, None);
    assert_eq!(
        controller.model().current_result.as_ref().map(|page| page.rows.len()),
        Some(1)
    );
    let label = controller.model().paginator_label.label();
    assert!(label.starts_with("Rows 1 – 1"));
    assert!(!label.contains(" of "), "{label}");
}

#[test]
#[ignore = "requires isolated GTK display"]
fn page_size_changes_apply_after_a_failed_first_load() {
    let (controller, outputs) = launch_tab(false);
    controller
        .sender()
        .send(BrowseTabInput::ShowError("connection lost".into()))
        .unwrap();
    controller.sender().send(BrowseTabInput::PageSizeChanged(500)).unwrap();
    pump(&outputs);
    assert_eq!(controller.model().page_size, 500);
}

#[test]
#[ignore = "requires isolated GTK display"]
fn custom_page_size_changes_the_exact_fetch_limit() {
    let (controller, outputs) = launch_tab(false);
    let bar = controller
        .model()
        .first_button
        .ancestor(gtk::ActionBar::static_type())
        .unwrap();
    let page_size = descendants(&bar)
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::SpinButton>().ok())
        .unwrap();

    page_size.set_value(2_347.0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut fetched = Vec::new();
    while controller.model().page_size != 2_347 && std::time::Instant::now() < deadline {
        fetched.extend(pump(&outputs));
    }

    assert_eq!(controller.model().page_size, 2_347);
    assert!(
        fetched
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::FetchPage))
    );
}

#[test]
#[ignore = "requires isolated GTK display"]
fn a_page_reload_retires_a_pending_cell_value_request() {
    let (controller, outputs) = launch_tab(false);
    let pending = controller.model().cell_value_requests.begin();
    assert!(controller.model().cell_value_requests.accepts(pending));
    controller.sender().send(BrowseTabInput::PageSizeChanged(500)).unwrap();
    pump(&outputs);
    assert_eq!(controller.model().page_size, 500);
    assert!(!controller.model().cell_value_requests.accepts(pending));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn read_only_selection_does_not_offer_delete() {
    let (controller, outputs) = launch_tab(true);
    load_two_rows(&controller, &outputs);
    controller.model().current_selection.as_ref().unwrap().select_all();
    pump(&outputs);
    let label = controller.model().selection_count_label.label();
    assert!(label.contains('2'));
    assert!(!label.contains("Delete"));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn load_error_page_retries_the_fetch() {
    let (controller, outputs) = launch_tab(false);
    controller
        .sender()
        .send(BrowseTabInput::ShowError("connection lost".into()))
        .unwrap();
    pump(&outputs);
    while outputs.try_recv().is_ok() {}
    let page = controller.model().inner_stack.child_by_name("error").unwrap();
    let retry = descendants(&page)
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::Button>().ok())
        .unwrap();
    retry.emit_clicked();
    let fetched = pump(&outputs);
    assert!(
        fetched
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::FetchPage))
    );
    assert!(
        fetched
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::FetchRowCount))
    );
}

#[test]
#[ignore = "requires isolated GTK display"]
fn paginator_controls_have_accessible_names() {
    let (controller, _outputs) = launch_tab(false);
    let bar = controller
        .model()
        .first_button
        .ancestor(gtk::ActionBar::static_type())
        .unwrap();
    let controls = descendants(&bar);
    let page_size = controls
        .iter()
        .find_map(|widget| widget.downcast_ref::<gtk::SpinButton>())
        .unwrap();
    let icon_buttons: Vec<_> = controls
        .iter()
        .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
        .filter(|button| button.icon_name().is_some())
        .filter(|button| button.ancestor(gtk::SpinButton::static_type()).is_none())
        .collect();
    assert!(icon_buttons.len() >= 6);
    for button in icon_buttons {
        assert!(
            gtk::test_accessible_has_property(button, gtk::AccessibleProperty::Label),
            "{:?} has no accessible label",
            button.icon_name()
        );
    }
    assert_eq!(page_size.adjustment().lower(), 1.0);
    assert_eq!(page_size.adjustment().upper(), 5_000.0);
    assert!(gtk::test_accessible_has_relation(
        page_size,
        gtk::AccessibleRelation::LabelledBy
    ));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn an_edit_for_a_saved_row_does_not_land_on_a_draft_inserted_at_its_position() {
    let (controller, outputs) = launch_tab(false);
    load_two_rows(&controller, &outputs);
    assert!(controller.model().row_position_still_holds(0, &[Value::Int(1)]));
    controller.sender().send(BrowseTabInput::InsertRow).unwrap();
    pump(&outputs);
    assert!(!controller.model().row_position_still_holds(0, &[Value::Int(1)]));
}

#[test]
#[ignore = "requires isolated GTK display"]
fn focus_grid_targets_the_current_browse_view() {
    let (controller, outputs) = launch_tab(false);
    let window = gtk::Window::builder().child(controller.widget()).build();
    window.present();
    load_two_rows(&controller, &outputs);
    let view = controller.model().current_column_view.clone().unwrap();
    controller.sender().send(BrowseTabInput::FocusGrid).unwrap();
    pump(&outputs);
    let focused = gtk::prelude::RootExt::focus(&window).unwrap();
    assert!(focused == *view.upcast_ref::<gtk::Widget>() || focused.is_ancestor(&view));
    window.destroy();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn an_estimated_row_count_updates_the_paginator_with_tilde() {
    let (controller, outputs) = launch_tab(false);
    load_two_rows(&controller, &outputs);
    let count_request = controller.model().row_count_requests.begin();
    controller
        .sender()
        .send(BrowseTabInput::RowCountLoaded {
            request: count_request,
            count: 10_000,
            is_estimated: true,
        })
        .unwrap();
    pump(&outputs);
    assert_eq!(controller.model().current_total_rows, Some(10_000));
    assert!(controller.model().is_count_estimated);
    assert!(controller.model().paginator_label.label().contains("~10,000"));
}
