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
                finishing: false,
                running: false,
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
    wait_for(&view, cx, |view| {
        view.transaction.as_ref().is_some_and(|t| t.statements == 1)
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let index = view
                .tabs
                .items
                .iter()
                .position(|tab| *tab == bookie_gpui::shell::Tab::Query)
                .expect("a query tab");
            view.close_tab(index);
            assert!(
                view.tabs.items.contains(&bookie_gpui::shell::Tab::Query),
                "{}",
                view.status
            );
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    assert_eq!(
        saved_customer(&path, 1),
        original,
        "a rolled back update leaves the row alone"
    );

    open_transaction(&view, cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.run_in_transaction("UPDATE orders SET customer = 'Committed' WHERE id = 1".into(), cx)
        })
    });
    wait_for(&view, cx, |view| {
        view.transaction.as_ref().is_some_and(|t| t.statements == 1)
    });
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
            .run_in_transaction(
                &shared,
                "UPDATE orders SET customer = 'Nope' WHERE id = 1",
                &tokio_util::sync::CancellationToken::new()
            )
            .is_err(),
        "a read-only connection refuses writes inside a transaction"
    );
    write.rollback_transaction(&shared).expect("roll back");
    assert_eq!(saved_customer(&path, 1), original);
    let _ = (view, cx);
}

#[gpui::test]
fn replacing_a_load_does_not_leave_a_transaction_statement_running(cx: &mut TestAppContext) {
    let path = fixture_copy("transaction-replaced-load");
    let (view, cx) = saving_view(cx, &path);
    let original = saved_customer(&path, 1);
    open_transaction(&view, cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.run_in_transaction("UPDATE orders SET customer = 'Temporary' WHERE id = 1".into(), cx);
            assert!(view.transaction.as_ref().is_some_and(|transaction| transaction.running));
            view.invalidate_background();
        })
    });
    wait_for(&view, cx, |view| view.transaction.as_ref().is_some_and(|transaction| !transaction.running));
    view.read_with(cx, |view, _| {
        assert!(view.transaction.as_ref().is_some_and(|transaction| !transaction.running));
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    assert_eq!(saved_customer(&path, 1), original);
    std::fs::remove_file(path).ok();
}

#[derive(Clone, Copy)]
enum CommitResult {
    Denied,
    RolledBack,
    Unknown,
    CommittedCloseFails,
}

struct DeniedCommit {
    result: CommitResult,
    rolled_back: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait::async_trait]
impl bookie_core::Session for DeniedCommit {
    async fn query_params_controlled(
        &mut self,
        sql: &str,
        _: &[bookie_core::Value],
        _: &bookie_core::OperationControl,
    ) -> Result<bookie_core::QueryResult, bookie_core::DriverError> {
        assert_eq!(sql, "ROLLBACK");
        self.rolled_back.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(bookie_core::QueryResult {
            columns: Vec::new(),
            rows: Vec::new().into(),
            truncated: false,
        })
    }
    async fn commit_controlled(
        &mut self,
        _: &bookie_core::OperationControl,
    ) -> Result<bookie_core::SessionCommitOutcome, bookie_core::DriverError> {
        match self.result {
            CommitResult::Denied => Err(bookie_core::DriverError::PolicyDenied("commit denied".into())),
            CommitResult::RolledBack => Ok(bookie_core::SessionCommitOutcome::RolledBack),
            CommitResult::Unknown => Err(bookie_core::DriverError::OperationOutcomeUnknown {
                source: Box::new(bookie_core::DriverError::TimedOut),
            }),
            CommitResult::CommittedCloseFails => Ok(bookie_core::SessionCommitOutcome::Committed),
        }
    }
    fn is_usable(&self) -> bool {
        true
    }
    fn transaction_open(&self) -> bool {
        true
    }
    async fn close(self: Box<Self>) -> Result<(), bookie_core::DriverError> {
        if matches!(self.result, CommitResult::CommittedCloseFails) {
            Err(bookie_core::DriverError::Disconnected)
        } else {
            Ok(())
        }
    }
}

#[gpui::test]
fn a_denied_commit_keeps_the_transaction_available_for_rollback(cx: &mut TestAppContext) {
    let path = fixture_copy("denied-transaction-commit");
    let (view, cx) = saving_view(cx, &path);
    let rolled_back = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shared = std::sync::Arc::new(tokio::sync::Mutex::new(Some(
        bookie_app_services::interactive_session::SessionHandle::plain(Box::new(DeniedCommit {
            result: CommitResult::Denied,
            rolled_back: rolled_back.clone(),
        })),
    )));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.transaction = Some(crate::transaction_ui::TransactionState {
                write: write_session(&path, false),
                shared,
                started: Instant::now(),
                last_activity: Instant::now(),
                statements: 1,
                finishing: false,
                running: false,
            });
            view.transaction.as_mut().unwrap().running = true;
            view.end_transaction(true, cx);
            assert!(view.status.contains("Wait for the transaction operation"));
            view.transaction.as_mut().unwrap().running = false;
            view.end_transaction(true, cx);
        })
    });
    wait_for(&view, cx, |view| view.status.starts_with("Commit failed:"));
    view.read_with(cx, |view, _| {
        assert!(view.transaction_open(), "denied commit must keep rollback available")
    });
    assert!(!cx.simulate_close(), "an open transaction must prevent window close");
    cx.update(|_, cx| view.update(cx, |view, cx| view.disconnect(cx)));
    view.read_with(cx, |view, _| {
        assert!(view.transaction_open());
        assert!(view.status.contains("before disconnecting"));
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connect(bookie_gpui::live::Origin::File(path.clone()), cx);
            assert!(view.transaction_open());
            assert!(view.status.contains("before switching connections"));
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    assert!(rolled_back.load(std::sync::atomic::Ordering::SeqCst));
    std::fs::remove_file(path).ok();
}

#[gpui::test]
fn terminal_commit_outcomes_are_not_reported_as_successful_retries(cx: &mut TestAppContext) {
    for (result, expected) in [
        (CommitResult::RolledBack, "Transaction rolled back by the server"),
        (CommitResult::Unknown, "Transaction outcome may be unknown"),
        (
            CommitResult::CommittedCloseFails,
            "Transaction committed; closing the session failed",
        ),
    ] {
        let path = fixture_copy("transaction-outcome");
        let (view, cx) = saving_view(cx, &path);
        let shared = std::sync::Arc::new(tokio::sync::Mutex::new(Some(
            bookie_app_services::interactive_session::SessionHandle::plain(Box::new(DeniedCommit {
                result,
                rolled_back: Default::default(),
            })),
        )));
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.transaction = Some(crate::transaction_ui::TransactionState {
                    write: write_session(&path, false),
                    shared: shared.clone(),
                    started: Instant::now(),
                    last_activity: Instant::now(),
                    statements: 1,
                    finishing: false,
                    running: false,
                });
                view.end_transaction(true, cx);
                assert!(view.transaction_open());
                view.end_transaction(false, cx);
            })
        });
        wait_for(&view, cx, |view| view.status.starts_with(expected));
        view.read_with(cx, |view, _| {
            assert!(view.status.starts_with(expected), "{}", view.status);
            assert!(!view.transaction_open());
        });
        assert!(
            shared.try_lock().unwrap().is_none(),
            "terminal operation must release its session"
        );
        std::fs::remove_file(path).ok();
    }
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
