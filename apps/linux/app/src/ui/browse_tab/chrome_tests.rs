use relm4::gtk::prelude::*;
use relm4::{Component, ComponentController, Controller, gtk};
use tablepro_core::{ColumnInfo, QueryResult, Value};
use uuid::Uuid;

use super::{BrowseTab, BrowseTabInit, BrowseTabInput, BrowseTabOutput};

fn primary_key_column() -> ColumnInfo {
    ColumnInfo {
        name: "id".into(),
        data_type: "integer".into(),
        nullable: false,
        primary_key: true,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn launch_tab(read_only: bool) -> (Controller<BrowseTab>, std::sync::mpsc::Receiver<BrowseTabOutput>) {
    relm4::gtk::init().unwrap();
    let (output_sender, output_receiver) = relm4::channel::<BrowseTabOutput>();
    let (observed_sender, observed_receiver) = std::sync::mpsc::channel();
    relm4::spawn_local(async move {
        while let Some(output) = output_receiver.recv().await {
            if observed_sender.send(output).is_err() {
                break;
            }
        }
    });
    let controller = BrowseTab::builder()
        .launch(BrowseTabInit {
            tab_id: Uuid::new_v4(),
            schema: Some("main".into()),
            table: "items".into(),
            driver_id: "sqlite".into(),
            connection_id: None,
            read_only,
            page_size: 100,
            initial_offset: 0,
            initial_sort: None,
            persistence: crate::services::persistence_stores::PersistenceStores {
                column_widths: None,
                filter_settings: None,
                csv_import_mappings: None,
                named_filter_presets: None,
            },
            database: std::sync::Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
        })
        .forward(&output_sender, |output| output);
    (controller, observed_receiver)
}

fn settle() {
    let context = relm4::gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(50);
    while std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

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

fn load_two_rows(controller: &Controller<BrowseTab>) {
    controller
        .sender()
        .send(BrowseTabInput::ColumnsLoaded(vec![primary_key_column()]))
        .unwrap();
    settle();
    let request = controller.model().page_requests.begin(0);
    controller
        .sender()
        .send(BrowseTabInput::RowsLoaded {
            request,
            result: QueryResult {
                columns: Vec::new(),
                rows: vec![vec![Value::Int(1)], vec![Value::Int(2)]],
                truncated: false,
            },
            projected_columns: None,
        })
        .unwrap();
    settle();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn a_row_count_queued_before_refresh_is_not_shown() {
    let (controller, _outputs) = launch_tab(false);
    controller
        .sender()
        .send(BrowseTabInput::ColumnsLoaded(vec![primary_key_column()]))
        .unwrap();
    settle();
    let stale = controller.model().row_count_requests.begin();
    controller.sender().send(BrowseTabInput::Refresh).unwrap();
    controller
        .sender()
        .send(BrowseTabInput::RowCountLoaded {
            request: stale,
            count: 7,
        })
        .unwrap();
    settle();
    assert_eq!(controller.model().current_total_rows, None);
}

#[test]
#[ignore = "requires isolated GTK display"]
fn page_size_changes_apply_after_a_failed_first_load() {
    let (controller, _outputs) = launch_tab(false);
    controller
        .sender()
        .send(BrowseTabInput::ShowError("connection lost".into()))
        .unwrap();
    controller.sender().send(BrowseTabInput::PageSizeChanged(500)).unwrap();
    settle();
    assert_eq!(controller.model().page_size, 500);
}

#[test]
#[ignore = "requires isolated GTK display"]
fn read_only_selection_does_not_offer_delete() {
    let (controller, _outputs) = launch_tab(true);
    load_two_rows(&controller);
    controller.model().current_selection.as_ref().unwrap().select_all();
    settle();
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
    settle();
    while outputs.try_recv().is_ok() {}
    let page = controller.model().inner_stack.child_by_name("error").unwrap();
    let retry = descendants(&page)
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::Button>().ok())
        .unwrap();
    retry.emit_clicked();
    settle();
    let fetched: Vec<_> = outputs.try_iter().collect();
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
    let icon_buttons: Vec<_> = controls
        .iter()
        .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
        .filter(|button| button.icon_name().is_some())
        .collect();
    assert!(icon_buttons.len() >= 6);
    for button in icon_buttons {
        assert!(
            gtk::test_accessible_has_property(button, gtk::AccessibleProperty::Label),
            "{:?} has no accessible label",
            button.icon_name()
        );
    }
    let dropdown = controls
        .iter()
        .find_map(|widget| widget.downcast_ref::<gtk::DropDown>())
        .unwrap();
    assert!(gtk::test_accessible_has_relation(
        dropdown,
        gtk::AccessibleRelation::LabelledBy
    ));
}
