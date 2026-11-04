use super::save_in_flight_tests::{
    edit, launch_with_read_only, load_two_rows, pending, pump, transaction_save_id, transactions,
};
use super::{BrowseTabInput, BrowseTabOutput};
use relm4::ComponentController;
use uuid::Uuid;

fn launch(
    tab_id: Uuid,
) -> (
    relm4::Controller<super::BrowseTab>,
    std::sync::mpsc::Receiver<BrowseTabOutput>,
) {
    relm4::gtk::init().unwrap();
    launch_with_read_only(tab_id, false)
}

#[test]
#[ignore = "requires isolated GTK display"]
fn edits_and_a_second_save_are_ignored_until_the_save_in_flight_finishes() {
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch(tab_id);
    load_two_rows(&controller, &observed);

    edit(&controller, 1, "first");
    pump(&observed);
    assert_eq!(pending(&controller), 1);
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    let started = pump(&observed);
    assert_eq!(transactions(&started), 1, "the first save starts a transaction");
    let save_id = transaction_save_id(&started);

    edit(&controller, 2, "during the save");
    let during = pump(&observed);
    assert_eq!(pending(&controller), 1);
    assert!(
        during
            .iter()
            .any(|output| matches!(output, BrowseTabOutput::ShowToast(_)))
    );
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    assert_eq!(transactions(&pump(&observed)), 0);
    controller
        .sender()
        .send(BrowseTabInput::SaveFailed(save_id, "lost".into()))
        .unwrap();
    pump(&observed);
    edit(&controller, 2, "after the save");
    pump(&observed);
    assert_eq!(pending(&controller), 2);
    controller.detach_runtime();
}

#[test]
#[ignore = "requires isolated GTK display"]
fn an_old_save_completion_does_not_finish_a_newer_save() {
    let tab_id = Uuid::new_v4();
    let (mut controller, observed) = launch(tab_id);
    load_two_rows(&controller, &observed);

    edit(&controller, 1, "first save");
    pump(&observed);
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    let first_started = pump(&observed);
    assert_eq!(transactions(&first_started), 1);
    let first_save_id = transaction_save_id(&first_started);
    controller
        .sender()
        .send(BrowseTabInput::SaveCompleted(first_save_id))
        .unwrap();
    pump(&observed);

    edit(&controller, 1, "second save");
    pump(&observed);
    controller.sender().send(BrowseTabInput::CommitSave).unwrap();
    let second_started = pump(&observed);
    assert_eq!(transactions(&second_started), 1);
    let second_save_id = transaction_save_id(&second_started);
    controller
        .sender()
        .send(BrowseTabInput::SaveCompleted(first_save_id))
        .unwrap();
    pump(&observed);

    assert_eq!(pending(&controller), 1);
    assert!(controller.model().save_in_flight());
    controller
        .sender()
        .send(BrowseTabInput::SaveCompleted(second_save_id))
        .unwrap();
    pump(&observed);
    assert_eq!(pending(&controller), 0);
    controller.detach_runtime();
}
