use super::{BrowsePageRequest, BrowseTab, BrowseTabInit, BrowseTabInput, BrowseTabOutput};
use relm4::gtk::prelude::SelectionModelExt;
use relm4::{Component, ComponentController};
use std::sync::mpsc::Receiver;
use tablepro_core::{ColumnInfo, QueryResult, Value};
use uuid::Uuid;

fn column(name: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: if primary_key { "integer" } else { "text" }.into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn pump(outputs: &Receiver<BrowseTabOutput>) -> Vec<BrowseTabOutput> {
    let context = relm4::gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
    let mut seen = Vec::new();
    while std::time::Instant::now() < deadline {
        context.iteration(false);
        while let Ok(output) = outputs.try_recv() {
            seen.push(output);
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    seen
}

fn launch(tab_id: Uuid) -> (relm4::Controller<BrowseTab>, Receiver<BrowseTabOutput>) {
    launch_with_read_only(tab_id, false)
}

fn launch_with_read_only(tab_id: Uuid, read_only: bool) -> (relm4::Controller<BrowseTab>, Receiver<BrowseTabOutput>) {
    let (output_sender, output_receiver) = relm4::channel::<BrowseTabOutput>();
    let (observed_sender, observed) = std::sync::mpsc::channel();
    relm4::spawn_local(async move {
        while let Some(output) = output_receiver.recv().await {
            if observed_sender.send(output).is_err() {
                break;
            }
        }
    });
    let controller = BrowseTab::builder()
        .launch(BrowseTabInit {
            tab_id,
            schema: Some("public".into()),
            table: "notes".into(),
            driver_id: "postgres".into(),
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
    (controller, observed)
}

#[test]
#[ignore = "requires isolated GTK display"]
fn read_only_tab_rejects_mutations_from_toolbar_shortcuts_and_grid_dispatch() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch_with_read_only(tab_id, true);
    load_one_row(&controller, &observed);

    let selection = controller.model().current_selection.clone().unwrap();
    selection.select_item(0, false);
    controller.sender().send(BrowseTabInput::InsertRow).unwrap();
    controller
        .sender()
        .send(BrowseTabInput::DuplicateRow { row_position: 0 })
        .unwrap();
    controller.sender().send(BrowseTabInput::DeleteSelectedRow).unwrap();
    controller.sender().send(BrowseTabInput::Undo).unwrap();
    controller.sender().send(BrowseTabInput::Redo).unwrap();
    controller
        .sender()
        .send(BrowseTabInput::GridCellEdited {
            row_position: 0,
            col_index: 1,
            new_value: "changed".into(),
            value: None,
            row_key: vec![Value::Int(1)],
        })
        .unwrap();
    controller
        .sender()
        .send(BrowseTabInput::GridSetCellNull {
            row_position: 0,
            col_index: 1,
            row_key: vec![Value::Int(1)],
        })
        .unwrap();
    controller
        .sender()
        .send(BrowseTabInput::GridDeleteRowAt {
            row_position: 0,
            row_key: vec![Value::Int(1)],
        })
        .unwrap();
    controller
        .sender()
        .send(BrowseTabInput::Paste(super::paste::PasteAnchor {
            row: 0,
            col: 1,
            row_key: vec![Value::Int(1)],
            displayed: None,
        }))
        .unwrap();
    controller
        .sender()
        .send(BrowseTabInput::PasteText(super::paste::PasteTextInput {
            row: 0,
            col: 1,
            row_key: vec![Value::Int(1)],
            text: "changed".into(),
            displayed: None,
            snapshot: controller.model().capture_paste_snapshot(),
        }))
        .unwrap();
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    controller
        .model()
        .grid_sender
        .send(crate::ui::grid::GridMsg::InsertRow)
        .unwrap();
    controller
        .model()
        .grid_sender
        .send(crate::ui::grid::GridMsg::DuplicateRow { row_position: 0 })
        .unwrap();
    controller
        .model()
        .grid_sender
        .send(crate::ui::grid::GridMsg::DeleteRowAt {
            row_position: 0,
            row_key: vec![Value::Int(1)],
        })
        .unwrap();
    controller
        .model()
        .grid_sender
        .send(crate::ui::grid::GridMsg::SetCellNull {
            row_position: 0,
            col_index: 1,
            row_key: vec![Value::Int(1)],
        })
        .unwrap();
    controller
        .model()
        .grid_sender
        .send(crate::ui::grid::GridMsg::CellEdited {
            row_position: 0,
            col_index: 1,
            new_value: "grid edit".into(),
            row_key: vec![Value::Int(1)],
        })
        .unwrap();
    let outputs = pump(&observed);

    assert_eq!(
        pending(tab_id),
        0,
        "read-only actions must not create draft or row changes"
    );
    assert_eq!(transactions(&outputs), 0, "read-only actions must not dispatch a save");
    let refused = outputs
        .iter()
        .filter(|output| matches!(output, BrowseTabOutput::ShowToast(message) if message == "This connection is read-only."))
        .count();
    assert_eq!(refused, 16, "every read-only mutation message must be refused");
    controller.detach_runtime();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn read_only_tab_can_discard_a_pending_local_draft() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch_with_read_only(tab_id, true);
    crate::services::change_tracker::with_tab(tab_id, |tracker| tracker.track_insert(vec![Value::Null]))
        .expect("tab tracker exists");

    assert_eq!(pending(tab_id), 1);
    controller.sender().send(BrowseTabInput::DiscardAll).unwrap();
    let outputs = pump(&observed);

    assert_eq!(pending(tab_id), 0, "discarding a local draft must remain available");
    assert_eq!(
        transactions(&outputs),
        0,
        "discarding a local draft must not dispatch a database write"
    );
    assert!(!outputs.iter().any(
        |output| matches!(output, BrowseTabOutput::ShowToast(message) if message == "This connection is read-only.")
    ));
    controller.detach_runtime();
}

fn load_one_row(controller: &relm4::Controller<BrowseTab>, observed: &Receiver<BrowseTabOutput>) {
    let columns = vec![column("id", true), column("title", false), column("body", false)];
    controller
        .sender()
        .send(BrowseTabInput::ColumnsLoaded(columns.clone()))
        .unwrap();
    pump(observed);
    let request = BrowsePageRequest {
        id: controller
            .model()
            .page_requests
            .current_id()
            .expect("a page was requested"),
        offset: 0,
    };
    let rows = vec![vec![Value::Int(1), Value::Text("a".into()), Value::Text("b".into())]];
    controller
        .sender()
        .send(BrowseTabInput::RowsLoaded {
            request,
            result: QueryResult {
                columns,
                rows,
                truncated: false,
            },
            projected_columns: None,
        })
        .unwrap();
    pump(observed);
}

fn edit(controller: &relm4::Controller<BrowseTab>, col_index: usize, new_value: &str) {
    controller
        .sender()
        .send(BrowseTabInput::GridCellEdited {
            row_position: 0,
            col_index,
            new_value: new_value.into(),
            value: None,
            row_key: vec![Value::Int(1)],
        })
        .unwrap();
}

fn pending(tab_id: Uuid) -> usize {
    crate::services::change_tracker::with_tab_ref(tab_id, |tracker| tracker.pending_count()).unwrap_or(0)
}

fn transactions(outputs: &[BrowseTabOutput]) -> usize {
    outputs
        .iter()
        .filter(|output| matches!(output, BrowseTabOutput::ExecuteTransaction { .. }))
        .count()
}

#[test]
#[ignore = "requires isolated GTK display"]
fn edits_and_a_second_save_are_ignored_until_the_save_in_flight_finishes() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch(tab_id);
    load_one_row(&controller, &observed);

    edit(&controller, 1, "first");
    pump(&observed);
    assert_eq!(pending(tab_id), 1);
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    assert_eq!(transactions(&pump(&observed)), 1, "the first save starts a transaction");

    edit(&controller, 2, "during the save");
    let during = pump(&observed);
    assert_eq!(
        pending(tab_id),
        1,
        "an edit made while saving must not enter the tracker"
    );
    assert!(
        during
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::ShowToast(_)))
    );
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    assert_eq!(transactions(&pump(&observed)), 0, "a second save must not start");

    controller
        .sender()
        .send(BrowseTabInput::SaveFailed("lost".into()))
        .unwrap();
    pump(&observed);
    edit(&controller, 2, "after the save");
    pump(&observed);
    assert_eq!(pending(tab_id), 2, "edits are accepted again once the save has ended");
    controller.detach_runtime();
}
