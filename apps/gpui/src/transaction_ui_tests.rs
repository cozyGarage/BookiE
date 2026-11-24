use std::time::Instant;

use gpui::TestAppContext;

use crate::GridView;
use crate::save_ui_tests::{fixture_copy, saved_customer, saving_view, settle_saving, write_session};

fn wait_for(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, done: impl Fn(&GridView) -> bool) {
    for _ in 0..200 {
        settle_saving(cx);
        if view.read_with(cx, |view, _| done(view)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn open_transaction(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, path: &std::path::Path) {
    let write = write_session(path, false);
    let shared = write.begin_transaction().expect("begin a transaction");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.transaction = Some(crate::transaction_ui::TransactionState {
                write,
                shared,
                started: Instant::now(),
                last_activity: Instant::now(),
                statements: 0,
            });
            view.tabs.open_query();
            view.sync_grid_owner();
        })
    });
}

#[gpui::test]
fn a_transaction_rolls_back_or_commits_and_keeps_its_query_tab_open(cx: &mut TestAppContext) {
    let path = fixture_copy("transaction");
    let (view, cx) = saving_view(cx, &path);
    let original = saved_customer(&path, 1);

    open_transaction(&view, cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.run_in_transaction("UPDATE orders SET customer = 'Rolled' WHERE id = 1".into(), cx)
        })
    });
    wait_for(&view, cx, |view| view.transaction.as_ref().is_some_and(|t| t.statements == 1));
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let index = view
                .tabs
                .items
                .iter()
                .position(|tab| *tab == bookie_gpui::shell::Tab::Query)
                .expect("a query tab");
            view.close_tab(index);
            assert!(view.tabs.items.contains(&bookie_gpui::shell::Tab::Query), "{}", view.status);
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    assert_eq!(saved_customer(&path, 1), original, "a rolled back update leaves the row alone");

    open_transaction(&view, cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.run_in_transaction("UPDATE orders SET customer = 'Committed' WHERE id = 1".into(), cx)
        })
    });
    wait_for(&view, cx, |view| view.transaction.as_ref().is_some_and(|t| t.statements == 1));
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(true, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction committed");
    assert_eq!(saved_customer(&path, 1), "Committed");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_read_only_transaction_refuses_the_write_and_changes_nothing(cx: &mut TestAppContext) {
    let path = fixture_copy("transaction-read-only");
    let (view, cx) = saving_view(cx, &path);
    let original = saved_customer(&path, 1);
    let write = write_session(&path, true);
    let shared = write.begin_transaction().expect("begin a transaction");
    assert!(
        write
            .run_in_transaction(&shared, "UPDATE orders SET customer = 'Nope' WHERE id = 1", &tokio_util::sync::CancellationToken::new())
            .is_err(),
        "a read-only connection refuses writes inside a transaction"
    );
    write.rollback_transaction(&shared).expect("roll back");
    assert_eq!(saved_customer(&path, 1), original);
    let _ = (view, cx);
}

#[gpui::test]
fn a_transaction_idle_for_five_minutes_shows_the_shared_warning(cx: &mut TestAppContext) {
    let path = fixture_copy("transaction-idle");
    let (view, cx) = saving_view(cx, &path);
    open_transaction(&view, cx, &path);
    view.read_with(cx, |view, _| assert_eq!(view.transaction_idle_warning(), None));
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            if let Some(transaction) = view.transaction.as_mut() {
                transaction.last_activity = Instant::now() - std::time::Duration::from_secs(6 * 60);
            }
        })
    });
    view.read_with(cx, |view, _| {
        let warning = view.transaction_idle_warning().expect("an idle warning");
        assert!(warning.starts_with("Idle for 6 min"), "{warning}");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    std::fs::remove_file(&path).ok();
}
