use super::*;

fn report(rows: &[(i64, &str, &str)]) -> bookie_app_services::grid_replace::ReplaceReport {
    let mut snapshot = bookie_app_services::grid_replace::ReplaceSnapshot::default();
    for (position, (id, title, body)) in rows.iter().enumerate() {
        let values = [Value::Text((*title).into()), Value::Text((*body).into())];
        snapshot
            .push(position as u32, &[Value::Int(*id)], &[(1, &values[0]), (2, &values[1])])
            .unwrap();
    }
    let query = bookie_app_services::grid_replace::ReplaceQuery {
        needle: ".".into(),
        options: bookie_app_services::text_match::TextMatchOptions {
            regex: true,
            ..Default::default()
        },
        column: None,
    };
    bookie_app_services::grid_replace::plan_snapshot_replacements(
        &snapshot,
        bookie_app_services::grid_replace::ReplaceRequest {
            query: &query,
            replacement: "x",
            editable: &[false, true, true],
        },
        &tokio_util::sync::CancellationToken::new(),
        |value| match value {
            Value::Text(value) => value.clone(),
            _ => String::new(),
        },
    )
    .unwrap()
}

fn apply(
    controller: &Controller<BrowseTab>,
    report: bookie_app_services::grid_replace::ReplaceReport,
) -> Result<
    bookie_app_services::grid_replace::ReplaceAcknowledgement,
    bookie_app_services::grid_replace::ReplaceApplyError,
> {
    let generation = controller.model().row_store().unwrap().generation();
    let cancel = tokio_util::sync::CancellationToken::new();
    let (reply, mut response) = tokio::sync::oneshot::channel();
    controller
        .sender()
        .send(BrowseTabInput::GridReplaceBatch {
            report,
            generation,
            cancel,
            reply,
        })
        .unwrap();
    let context = relm4::gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        context.iteration(false);
        match response.try_recv() {
            Ok(result) => return result,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => panic!("replace reply closed"),
        }
    }
    panic!("replace reply timed out");
}

#[test]
#[ignore = "requires isolated GTK display"]
fn stale_later_cell_rejects_the_full_replace_batch() {
    relm4::gtk::init().unwrap();
    let (mut controller, observed) = launch(Uuid::new_v4());
    load_two_rows(&controller, &observed);
    let planned = report(&[(1, "a", "b"), (2, "a", "d")]);
    assert!(matches!(
        apply(&controller, planned),
        Err(bookie_app_services::grid_replace::ReplaceApplyError::Stale)
    ));
    let pending = controller
        .model()
        .change_trackers
        .with_tab(controller.model().tab_id, |tracker| tracker.pending_count())
        .unwrap();
    assert_eq!(pending, 0);
    controller.detach_runtime();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn valid_replace_batch_acknowledges_all_staged_cells_once() {
    relm4::gtk::init().unwrap();
    let (mut controller, observed) = launch(Uuid::new_v4());
    load_two_rows(&controller, &observed);
    let planned = report(&[(1, "a", "b"), (2, "c", "d")]);
    let acknowledgement = apply(&controller, planned).unwrap();
    assert_eq!(acknowledgement.replacements, 4);
    let pending = controller
        .model()
        .change_trackers
        .with_tab(controller.model().tab_id, |tracker| tracker.pending_count())
        .unwrap();
    assert_eq!(pending, 4);
    controller.detach_runtime();
}
