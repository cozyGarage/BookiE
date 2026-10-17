use super::{BrowsePageRequest, BrowseTab, BrowseTabInit, BrowseTabInput, BrowseTabOutput};
use relm4::gtk::gio::prelude::ListModelExt;
use relm4::gtk::glib::prelude::Cast;
use relm4::gtk::prelude::{ButtonExt, SelectionModelExt, ShortcutActionExt, ShortcutTriggerExt, WidgetExt};
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
fn read_only_tab_rejects_mutations_from_toolbar_context_actions_and_bulk_selection() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch_with_read_only(tab_id, true);
    load_two_rows(&controller, &observed);

    let selection = controller.model().current_selection.clone().unwrap();
    selection.select_item(0, false);
    selection.select_item(1, true);
    assert!(!controller.model().insert_button.is_visible());
    controller.model().insert_button.emit_clicked();
    controller
        .sender()
        .send(BrowseTabInput::DuplicateRow { row_position: 0 })
        .unwrap();
    assert!(activate_shortcut(&controller, "Delete"));
    assert!(activate_shortcut(&controller, "<Primary>n"));
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
    controller.model().save_button.emit_clicked();
    assert!(
        controller
            .model()
            .current_column_view
            .as_ref()
            .expect("the loaded page has a grid")
            .activate_action("cell.insert-row", None)
            .is_ok()
    );
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
        .filter(|output| matches!(output, BrowseTabOutput::ShowToast(_)))
        .count();
    assert_eq!(refused, 17, "every read-only mutation message must be refused");
    controller.detach_runtime();
}

fn activate_shortcut(controller: &relm4::Controller<BrowseTab>, trigger_text: &str) -> bool {
    let widget = controller.widget();
    let target_trigger = crate::ui::shortcut::parse(trigger_text).to_str();
    let controllers = widget.observe_controllers();
    for controller_index in 0..controllers.n_items() {
        let Some(shortcut_controller) = controllers
            .item(controller_index)
            .and_then(|object| object.downcast::<relm4::gtk::ShortcutController>().ok())
        else {
            continue;
        };
        for shortcut_index in 0..shortcut_controller.n_items() {
            let Some(shortcut) = shortcut_controller
                .item(shortcut_index)
                .and_then(|object| object.downcast::<relm4::gtk::Shortcut>().ok())
            else {
                continue;
            };
            if shortcut
                .trigger()
                .is_some_and(|trigger| trigger.to_str() == target_trigger)
                && let Some(action) = shortcut.action()
            {
                return action.activate(
                    relm4::gtk::ShortcutActionFlags::EXCLUSIVE,
                    widget,
                    shortcut.arguments().as_ref(),
                );
            }
        }
    }
    false
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

#[test]
#[ignore = "requires isolated GTK display"]
fn keyless_tab_rejects_inline_edits_and_insert_actions() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch(tab_id);
    load_keyless_row(&controller, &observed);

    controller
        .sender()
        .send(BrowseTabInput::GridCellEdited {
            row_position: 0,
            col_index: 0,
            new_value: "changed".into(),
            value: None,
            row_key: Vec::new(),
        })
        .unwrap();
    controller.sender().send(BrowseTabInput::InsertRow).unwrap();
    let outputs = pump(&observed);

    assert_eq!(pending(tab_id), 0, "keyless mutation routes must not create drafts");
    assert_eq!(
        transactions(&outputs),
        0,
        "keyless mutation routes must not dispatch a save"
    );
    assert_eq!(
        outputs
            .iter()
            .filter(|output| matches!(output, BrowseTabOutput::ShowToast(_)))
            .count(),
        2,
        "each refused keyless mutation must explain why it was blocked"
    );
    controller.detach_runtime();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn keyless_tab_refuses_to_stage_deletion_of_a_persisted_row() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch(tab_id);
    load_keyless_row(&controller, &observed);
    let selection = controller.model().current_selection.clone().unwrap();
    selection.select_item(0, false);

    controller.sender().send(BrowseTabInput::DeleteSelectedRow).unwrap();
    let outputs = pump(&observed);

    assert!(
        outputs
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::ShowSelectionAlert { .. }))
    );
    assert_eq!(
        pending(tab_id),
        0,
        "a persisted keyless row cannot enter the change tracker"
    );
    assert_eq!(
        transactions(&outputs),
        0,
        "a refused keyless delete cannot dispatch a save"
    );
    assert!(
        selection.is_selected(0),
        "refusing deletion preserves the row selection"
    );
    controller.detach_runtime();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn generated_column_edit_and_null_messages_do_not_stage_mutations() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch(tab_id);
    load_generated_column_row(&controller, &observed);

    controller
        .sender()
        .send(BrowseTabInput::GridCellEdited {
            row_position: 0,
            col_index: 1,
            new_value: "forged value".into(),
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
    let outputs = pump(&observed);

    assert_eq!(pending(tab_id), 0, "generated cells must not enter the change tracker");
    assert_eq!(transactions(&outputs), 0, "generated cells must not dispatch a save");
    assert!(
        outputs
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::ShowToast(_)))
    );
    let selection = controller.model().current_selection.clone().unwrap();
    let row = selection
        .model()
        .unwrap()
        .item(0)
        .unwrap()
        .downcast::<crate::ui::row_object::RowObject>()
        .unwrap();
    assert_eq!(row.cell_value(1), Value::Text("database value".into()));
    controller.detach_runtime();
}

fn load_two_rows(controller: &relm4::Controller<BrowseTab>, observed: &Receiver<BrowseTabOutput>) {
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
    let rows = vec![
        vec![Value::Int(1), Value::Text("a".into()), Value::Text("b".into())],
        vec![Value::Int(2), Value::Text("c".into()), Value::Text("d".into())],
    ];
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

fn load_keyless_row(controller: &relm4::Controller<BrowseTab>, observed: &Receiver<BrowseTabOutput>) {
    let columns = vec![column("title", false)];
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
    controller
        .sender()
        .send(BrowseTabInput::RowsLoaded {
            request,
            result: QueryResult {
                columns,
                rows: vec![vec![Value::Text("original".into())]],
                truncated: false,
            },
            projected_columns: None,
        })
        .unwrap();
    pump(observed);
}

fn load_generated_column_row(controller: &relm4::Controller<BrowseTab>, observed: &Receiver<BrowseTabOutput>) {
    let columns = vec![column("id", true), generated_column("computed")];
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
    controller
        .sender()
        .send(BrowseTabInput::RowsLoaded {
            request,
            result: QueryResult {
                columns,
                rows: vec![vec![Value::Int(1), Value::Text("database value".into())]],
                truncated: false,
            },
            projected_columns: None,
        })
        .unwrap();
    pump(observed);
}

fn generated_column(name: &str) -> ColumnInfo {
    let mut column = column(name, false);
    column.is_generated = true;
    column
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
    load_two_rows(&controller, &observed);

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
