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
fn typed_transaction_end_clears_the_toolbar(cx: &mut TestAppContext) {
    for sql in ["COMMIT", "ROLLBACK"] {
        let path = fixture_copy("typed-transaction-end");
        let (view, cx) = saving_view(cx, &path);
        open_transaction(&view, cx, &path);
        view.update(cx, |view, cx| view.run_in_transaction(sql.into(), cx));
        wait_for(&view, cx, |view| view.transaction.is_none());
        view.read_with(cx, |view, _| {
            assert!(view.transaction.is_none(), "{sql} left the transaction toolbar open");
            assert!(!view.status.contains("not committed yet"), "{}", view.status);
        });
    }
}

#[gpui::test]
fn a_starting_transaction_blocks_closing_and_a_second_begin(cx: &mut TestAppContext) {
    let path = fixture_copy("transaction-starting");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.transaction_starting = true;
            assert!(view.transaction_open());
            view.begin_transaction(cx);
            assert!(view.transaction_starting);
            assert!(view.transaction.is_none());
        })
    });
    std::fs::remove_file(path).ok();
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
        view.update(cx, |view, cx| {
            let index = view
                .tabs
                .items
                .iter()
                .position(|tab| *tab == bookie_gpui::shell::Tab::Query)
                .expect("a query tab");
            view.close_tab(index, cx);
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
fn a_transaction_statement_survives_unrelated_load_invalidation(cx: &mut TestAppContext) {
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
    wait_for(&view, cx, |view| {
        view.transaction
            .as_ref()
            .is_some_and(|transaction| !transaction.running)
    });
    view.read_with(cx, |view, _| {
        assert!(
            view.transaction
                .as_ref()
                .is_some_and(|transaction| transaction.statements == 1),
            "the statement should finish despite the unrelated invalidation"
        );
        assert_eq!(view.status, "Ran in the transaction (not committed yet)");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    assert_eq!(saved_customer(&path, 1), original);
    std::fs::remove_file(path).ok();
}

#[gpui::test]
fn grid_save_requests_are_refused_inside_a_manual_transaction(cx: &mut TestAppContext) {
    let path = fixture_copy("transaction-blocks-save-request");
    let original = saved_customer(&path, 1);
    let (view, cx) = saving_view(cx, &path);
    open_transaction(&view, cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let index = view
                .tabs
                .items
                .iter()
                .position(|tab| matches!(tab, bookie_gpui::shell::Tab::Table(_)))
                .expect("the table tab remains open");
            view.tabs.active = index;
            view.sync_grid_owner();
            cx.notify();
        })
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit
                .edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("Blocked".into()));
            view.request_save(cx);
        })
    });
    wait_for(&view, cx, |view| !view.save.busy);
    view.read_with(cx, |view, _| {
        assert!(view.save.pending.is_none());
        assert!(view.status.contains("transaction"), "{}", view.status);
    });
    assert_eq!(saved_customer(&path, 1), original);
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    std::fs::remove_file(path).ok();
}

#[gpui::test]
fn a_reviewed_grid_save_cannot_write_outside_a_transaction_that_opened_later(cx: &mut TestAppContext) {
    let path = fixture_copy("transaction-blocks-reviewed-save");
    let original = saved_customer(&path, 1);
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit
                .edits
                .insert((0, 1), bookie_gpui::model::Cell::Text("Blocked".into()));
            view.request_save(cx);
        })
    });
    wait_for(&view, cx, |view| view.save.pending.is_some());

    let write = write_session(&path, false);
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
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    wait_for(&view, cx, |view| !view.save.busy);
    view.read_with(cx, |view, _| {
        assert!(view.save.pending.is_some(), "the reviewed save stays pending");
        assert!(view.status.contains("transaction"), "{}", view.status);
    });
    assert_eq!(saved_customer(&path, 1), original);
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
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
        assert!(view.transaction_open(), "denied commit must keep rollback available");
        let notice = view.notices.items.first().expect("a failed commit leaves a notice");
        assert!(notice.text.starts_with("Commit failed:") && !notice.read);
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
    use crate::notifications::NoticeKind;
    for (result, expected, notice) in [
        (
            CommitResult::RolledBack,
            "Transaction rolled back by the server",
            Some(NoticeKind::Error),
        ),
        (
            CommitResult::Unknown,
            "Transaction outcome may be unknown",
            Some(NoticeKind::UnknownOutcome),
        ),
        (
            CommitResult::CommittedCloseFails,
            "Transaction committed; closing the session failed",
            None,
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
            assert_eq!(view.notices.items.first().map(|item| item.kind), notice);
            assert!(view.notices.items.iter().all(|item| !item.read));
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
    cx.update(|_, cx| view.update(cx, |view, cx| view.sync_idle_notice(cx)));
    cx.update(|_, cx| view.update(cx, |view, cx| view.sync_idle_notice(cx)));
    view.read_with(cx, |view, _| {
        let idle: Vec<_> = view
            .notices
            .items
            .iter()
            .filter(|notice| notice.kind == crate::notifications::NoticeKind::IdleTransaction)
            .collect();
        assert_eq!(idle.len(), 1, "one unread notice, not one per refresh");
        assert!(!idle[0].read);
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.status = "Clicked a cell".into()));
    view.read_with(cx, |view, _| {
        assert_eq!(view.notices.unread(), 1, "a status change keeps it")
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    cx.update(|_, cx| view.update(cx, |view, cx| view.sync_idle_notice(cx)));
    view.read_with(cx, |view, _| {
        assert_eq!(view.notices.unread(), 0, "ending the transaction resolves the notice")
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn a_transaction_statement_is_refused_while_a_schema_context_is_set(cx: &mut TestAppContext) {
    let path = fixture_copy("transaction-schema-context");
    let (view, cx) = saving_view(cx, &path);
    let original = saved_customer(&path, 1);
    open_transaction(&view, cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.schema_context = Some("main".into());
            view.run_sql_with_mapping(
                "UPDATE orders SET customer = 'Elsewhere' WHERE id = 1".into(),
                Vec::new(),
                None,
                None,
                cx,
            );
            let transaction = view.transaction.as_ref().expect("still open");
            assert!(!transaction.running, "{}", view.status);
            assert_eq!(transaction.statements, 0);
            assert!(view.status.contains("schema"), "{}", view.status);
        })
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.end_transaction(false, cx)));
    wait_for(&view, cx, |view| view.status == "Transaction rolled back");
    assert_eq!(saved_customer(&path, 1), original);
    std::fs::remove_file(path).ok();
}
