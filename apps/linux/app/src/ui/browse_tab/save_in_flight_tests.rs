use super::{BrowsePageRequest, BrowseTab, BrowseTabInit, BrowseTabInput, BrowseTabOutput};
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
            read_only: false,
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

fn queue_edit_and_flush(
    controller: &relm4::Controller<BrowseTab>,
    col_index: usize,
) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
    let (queue, revision) = controller.model().edit_flush_snapshot();
    queue
        .send(crate::ui::grid::GridMsg::CellEdited {
            row_position: 0,
            col_index,
            new_value: "changed".into(),
            row_key: vec![Value::Int(1)],
        })
        .unwrap();
    let (reply, receiver) = tokio::sync::oneshot::channel();
    queue
        .send(crate::ui::grid::GridMsg::EditsFlushed { revision, reply })
        .unwrap();
    receiver
}

fn wait_for_flush(mut reply: tokio::sync::oneshot::Receiver<Result<(), String>>) -> Result<(), String> {
    let context = relm4::gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match reply.try_recv() {
            Ok(result) => return result,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => panic!("the flush reply was dropped"),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
        }
        assert!(std::time::Instant::now() < deadline, "the flush did not finish");
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
#[ignore = "requires isolated GTK display"]
fn a_close_flush_waits_until_queued_edits_enter_the_tracker() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch(tab_id);
    load_one_row(&controller, &observed);
    let mut reply = queue_edit_and_flush(&controller, 1);
    assert!(matches!(
        reply.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    assert!(wait_for_flush(reply).is_ok());
    assert_eq!(pending(tab_id), 1);
    controller.detach_runtime();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn a_close_flush_reports_a_refused_queued_edit_without_staging_it() {
    relm4::gtk::init().unwrap();
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch(tab_id);
    load_one_row(&controller, &observed);
    let reply = queue_edit_and_flush(&controller, 0);
    assert!(wait_for_flush(reply).is_err());
    assert_eq!(pending(tab_id), 0);
    controller.detach_runtime();
}
