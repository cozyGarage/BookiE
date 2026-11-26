use std::sync::Arc;
use std::time::Instant;

use bookie_app_services::interactive_session::SharedSession;
use bookie_gpui::live::WriteSession;

use super::*;
use crate::surfaces::{ButtonKind, button};

pub(crate) struct TransactionState {
    pub write: Arc<WriteSession>,
    pub shared: SharedSession,
    pub started: Instant,
    pub last_activity: Instant,
    pub statements: usize,
    pub finishing: bool,
    pub running: Option<RunningStatement>,
}

pub(crate) struct RunningStatement {
    pub started: Instant,
    pub cancel: CancellationToken,
}

impl RunningStatement {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            cancel: CancellationToken::new(),
        }
    }
}

fn ended_status(failure: Option<&str>, cleanup: Option<String>) -> String {
    match (failure, cleanup) {
        (Some(message), None) => {
            format!("The statement failed and the transaction session ended, so it was rolled back: {message}")
        }
        (Some(message), Some(cleanup)) => format!(
            "The statement failed and the transaction session ended: {message}; closing it also failed: {cleanup}"
        ),
        (None, None) => "The transaction ended".into(),
        (None, Some(cleanup)) => format!("The transaction ended; closing its session failed: {cleanup}"),
    }
}

impl GridView {
    pub(crate) fn transaction_open(&self) -> bool {
        self.transaction_starting || self.transaction.is_some()
    }

    pub(crate) fn begin_transaction(&mut self, cx: &mut Context<Self>) {
        if self.transaction_open() {
            return;
        }
        if self.save.busy {
            self.status = "Wait for the save to finish before starting a transaction".into();
            return;
        }
        if self.save.pending.is_some() || self.form_open() || self.modal_open() {
            self.status = "Finish or cancel the open form or save review before starting a transaction".into();
            return;
        }
        let Some((connection_id, opened_with)) = self
            .live
            .as_ref()
            .and_then(|live| live.saved.as_ref().map(|(id, _)| (*id, live.stamp.clone())))
        else {
            self.status = "Transactions need a saved connection".into();
            return;
        };
        let approvals = self.approval.tx.clone();
        self.status = "Starting a transaction…".into();
        self.transaction_starting = true;
        self.invalidate_background();
        cx.spawn(async move |this, cx| {
            let mut opened = Some(
                cx.background_executor()
                    .spawn(async move {
                        let write = Arc::new(WriteSession::open_saved(
                            connection_id,
                            approvals,
                            opened_with.as_deref(),
                        )?);
                        let shared = write.begin_transaction()?;
                        Ok::<_, String>((write, shared))
                    })
                    .await,
            );
            let applied = this.update(cx, |this, cx| {
                this.transaction_starting = false;
                match opened.take() {
                    Some(Ok((write, shared))) => {
                        this.transaction = Some(TransactionState {
                            write,
                            shared,
                            started: Instant::now(),
                            last_activity: Instant::now(),
                            statements: 0,
                            finishing: false,
                            running: None,
                        });
                        this.refresh_while_transaction_open(cx);
                        this.status = "Transaction open: statements run in it until you commit or roll back".into();
                    }
                    Some(Err(message)) => this.status = format!("Could not start a transaction: {message}"),
                    None => return,
                }
                cx.notify();
            });
            if applied.is_err()
                && let Some(Ok((write, shared))) = opened
            {
                cx.background_executor()
                    .spawn(async move { write.close_transaction(&shared) })
                    .await;
            }
        })
        .detach();
    }

    pub(crate) fn run_in_transaction(&mut self, sql: String, cx: &mut Context<Self>) {
        if self.transaction_starting {
            self.status = "Wait for the transaction to start".into();
            return;
        }
        let (Some(transaction), Some(live)) = (self.transaction.as_mut(), self.live.clone()) else {
            return;
        };
        if transaction.finishing {
            self.status = "Wait for the transaction to finish".into();
            return;
        }
        if transaction.running.is_some() {
            self.status = "Wait for the transaction statement to finish".into();
            return;
        }
        let statement = RunningStatement::start();
        let cancel = statement.cancel.clone();
        transaction.running = Some(statement);
        let (write, shared) = (transaction.write.clone(), transaction.shared.clone());
        self.status = "Running in the transaction…".into();
        self.invalidate_background();
        let owner = shared.clone();
        cx.spawn(async move |this, cx| {
            let (ran, ended, cleanup) = cx
                .background_executor()
                .spawn(async move {
                    let ran = write
                        .run_in_transaction(&shared, &sql, &cancel)
                        .and_then(|result| live.table_from_result(result, &sql, &cancel));
                    let ended = !write.transaction_is_open(&shared);
                    let cleanup = if ended { write.close_transaction(&shared) } else { None };
                    (ran, ended, cleanup)
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_transaction_statement(&owner, ran, ended.then_some(cleanup));
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.tick_transaction_statement(cx);
    }

    fn finish_transaction_statement(
        &mut self,
        owner: &SharedSession,
        ran: Result<bookie_gpui::live::LiveTable, String>,
        ended: Option<Option<String>>,
    ) {
        let Some(transaction) = self.transaction.as_mut().filter(|t| Arc::ptr_eq(&t.shared, owner)) else {
            return;
        };
        let cancelled = transaction
            .running
            .take()
            .is_some_and(|statement| statement.cancel.is_cancelled());
        if let Some(cleanup) = ended {
            self.transaction = None;
            self.status = ended_status(ran.as_ref().err().map(String::as_str), cleanup);
            return;
        }
        if ran.is_ok() {
            transaction.statements += 1;
            transaction.last_activity = Instant::now();
        }
        match ran {
            Ok(table) => {
                self.add_run(Some(Arc::new(table)));
                self.status = "Ran in the transaction (not committed yet)".into();
            }
            Err(message) if cancelled => {
                self.status = format!("The transaction statement was cancelled: {message}");
            }
            Err(message) => self.status = format!("The statement failed inside the transaction: {message}"),
        }
    }

    fn tick_transaction_statement(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
                let running = this
                    .update(cx, |view, cx| {
                        let running = view.transaction_statement_running();
                        if running && matches!(view.tabs.current(), Tab::Query) {
                            cx.notify();
                        }
                        running
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn transaction_statement_running(&self) -> bool {
        self.transaction
            .as_ref()
            .is_some_and(|transaction| transaction.running.is_some())
    }

    pub(crate) fn transaction_statement(&self) -> Option<&RunningStatement> {
        self.transaction.as_ref()?.running.as_ref()
    }

    pub(crate) fn stop_transaction_statement(&mut self) -> bool {
        let Some(cancel) = self.transaction_statement().map(|statement| statement.cancel.clone()) else {
            return false;
        };
        if !cancel.is_cancelled() {
            cancel.cancel();
            self.status = "Cancellation requested…".into();
        }
        true
    }

    pub(crate) fn end_transaction(&mut self, commit: bool, cx: &mut Context<Self>) {
        let Some(transaction) = self.transaction.as_mut() else {
            return;
        };
        if transaction.finishing || transaction.running.is_some() {
            self.status = "Wait for the transaction operation to finish".into();
            return;
        }
        transaction.finishing = true;
        let (write, shared) = (transaction.write.clone(), transaction.shared.clone());
        self.status = if commit { "Committing…" } else { "Rolling back…" }.into();
        cx.spawn(async move |this, cx| {
            let (result, open, cleanup) = cx
                .background_executor()
                .spawn(async move {
                    let result = if commit {
                        write
                            .commit_transaction(&shared)
                            .map(|(outcome, close)| (Some(outcome), close))
                    } else {
                        write
                            .rollback_transaction(&shared)
                            .map(|()| (None, None))
                            .map_err(bookie_core::DriverError::Internal)
                    };
                    let open = write.transaction_is_open(&shared);
                    let keep = commit && open && matches!(&result, Err(bookie_core::DriverError::PolicyDenied(_)));
                    let cleanup = if result.is_err() && !keep {
                        write.close_transaction(&shared)
                    } else {
                        None
                    };
                    (result, open, cleanup)
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok((outcome, close)) => {
                        this.transaction = None;
                        let message = match outcome {
                            Some(bookie_core::SessionCommitOutcome::Committed) => "Transaction committed",
                            Some(bookie_core::SessionCommitOutcome::RolledBack) => {
                                "Transaction rolled back by the server"
                            }
                            None => "Transaction rolled back",
                        };
                        this.status = match close {
                            Some(error) => format!("{message}; closing the session failed: {error}"),
                            None => message.into(),
                        };
                        if matches!(outcome, Some(bookie_core::SessionCommitOutcome::RolledBack)) {
                            this.push_notice(this.status.clone(), crate::notifications::NoticeKind::Error, cx);
                        }
                    }
                    Err(bookie_core::DriverError::PolicyDenied(message)) if commit && open => {
                        if let Some(transaction) = this.transaction.as_mut() {
                            transaction.finishing = false;
                        }
                        this.status = format!("Commit failed: {message}");
                        this.push_notice(this.status.clone(), crate::notifications::NoticeKind::Error, cx);
                    }
                    Err(error) => {
                        this.transaction = None;
                        this.status =
                            format!("Transaction outcome may be unknown: {error}; check the database before retrying");
                        if let Some(cleanup) = cleanup {
                            this.status
                                .push_str(&format!("; closing the session failed: {cleanup}"));
                        }
                        this.push_notice(
                            this.status.clone(),
                            crate::notifications::NoticeKind::UnknownOutcome,
                            cx,
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_while_transaction_open(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(30)).await;
                let open = this
                    .update(cx, |this, cx| {
                        this.sync_idle_notice(cx);
                        cx.notify();
                        this.transaction.is_some()
                    })
                    .unwrap_or(false);
                if !open {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn sync_idle_notice(&mut self, cx: &mut Context<Self>) {
        use crate::notifications::NoticeKind;
        let present = self
            .notices
            .items
            .iter()
            .any(|notice| notice.kind == NoticeKind::IdleTransaction);
        match self.transaction_idle_warning() {
            Some(text) if !present => self.push_notice(text, NoticeKind::IdleTransaction, cx),
            None if present => self
                .notices
                .items
                .retain(|notice| notice.kind != NoticeKind::IdleTransaction),
            _ => {}
        }
    }

    pub(crate) fn transaction_idle_warning(&self) -> Option<String> {
        self.transaction.as_ref().and_then(|transaction| {
            bookie_app_services::transaction_idle::idle_warning(transaction.last_activity.elapsed())
        })
    }

    pub(crate) fn transaction_controls(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let Some(transaction) = self.transaction.as_ref() else {
            return div().child(
                button(ButtonKind::Ghost, "begin-transaction", "Begin transaction", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.begin_transaction(cx);
                        cx.notify();
                    },
                )),
            );
        };
        let elapsed = transaction.started.elapsed().as_secs();
        let warning = self.transaction_idle_warning();
        let detail = format!(
            "Transaction · {}:{:02} · {} run{}{}",
            elapsed / 60,
            elapsed % 60,
            transaction.statements,
            if transaction.statements == 1 { "" } else { "s" },
            warning
                .as_ref()
                .map_or_else(String::new, |warning| format!(" · {warning}")),
        );
        div()
            .flex()
            .items_center()
            .gap_2()
            .child({
                let color = if warning.is_some() {
                    theme.destructive
                } else {
                    theme.warning
                };
                div()
                    .id("transaction-badge")
                    .debug_selector(|| "transaction-badge".into())
                    .px_2()
                    .rounded(px(theme.radius))
                    .border_1()
                    .border_color(rgb(color))
                    .text_color(rgb(color))
                    .tooltip({
                        let detail = detail.clone();
                        move |window, cx| gpui_component::tooltip::Tooltip::new(detail.clone()).build(window, cx)
                    })
                    .child(if compact {
                        format!("Tx · {}:{:02}", elapsed / 60, elapsed % 60)
                    } else {
                        detail
                    })
            })
            .child(
                button(ButtonKind::Ghost, "commit-transaction", "Commit", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.end_transaction(true, cx);
                        cx.notify();
                    },
                )),
            )
            .child(
                button(ButtonKind::Ghost, "rollback-transaction", "Roll back", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.end_transaction(false, cx);
                        cx.notify();
                    },
                )),
            )
            .when_some(transaction.running.as_ref(), |bar, statement| {
                if statement.cancel.is_cancelled() {
                    bar.child(crate::surfaces::unavailable_button(
                        ButtonKind::Ghost,
                        "stop-transaction-statement",
                        "Stopping…",
                        "Waiting for the database",
                        theme,
                    ))
                } else {
                    bar.child(
                        button(ButtonKind::Destructive, "stop-transaction-statement", "Stop", theme).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.stop_transaction_statement();
                                cx.notify();
                            }),
                        ),
                    )
                }
            })
    }
}
