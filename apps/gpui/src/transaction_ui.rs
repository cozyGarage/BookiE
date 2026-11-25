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
    pub running: bool,
}

impl GridView {
    pub(crate) fn transaction_open(&self) -> bool {
        self.transaction_starting || self.transaction.is_some()
    }

    pub(crate) fn begin_transaction(&mut self, cx: &mut Context<Self>) {
        if self.transaction_open() {
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
                            running: false,
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
        let (Some(transaction), Some(live)) = (self.transaction.as_ref(), self.live.clone()) else {
            return;
        };
        if transaction.finishing {
            self.status = "Wait for the transaction to finish".into();
            return;
        }
        if transaction.running {
            self.status = "Wait for the transaction statement to finish".into();
            return;
        }
        let (write, shared) = (transaction.write.clone(), transaction.shared.clone());
        if let Some(transaction) = self.transaction.as_mut() {
            transaction.running = true;
        }
        self.status = "Running in the transaction…".into();
        self.invalidate_background();
        let generation = self.loads.generation;
        let cancel = CancellationToken::new();
        self.loads.cancel = cancel.clone();
        let owner = shared.clone();
        cx.spawn(async move |this, cx| {
            let (ran, ended, cleanup) = cx.background_executor().spawn(async move {
                let ran = (|| -> Result<_, String> {
                    let result = write.run_in_transaction(&shared, &sql, &cancel)?;
                    live.table_from_result(result, &sql, &cancel)
                })();
                let ended = !write.transaction_is_open(&shared);
                let cleanup = if ended { write.close_transaction(&shared) } else { None };
                (ran, ended, cleanup)
            }).await;
            this.update(cx, |this, cx| {
                let Some(transaction) = this.transaction.as_mut().filter(|t| Arc::ptr_eq(&t.shared, &owner)) else {
                    return;
                };
                if ended {
                    this.transaction = None;
                    this.status = match (&ran, cleanup) {
                        (Err(message), None) => format!(
                            "The statement failed and the transaction session ended, so it was rolled back: {message}"
                        ),
                        (Err(message), Some(cleanup)) => format!(
                            "The statement failed and the transaction session ended: {message}; closing it also failed: {cleanup}"
                        ),
                        (Ok(_), None) => "The transaction ended".into(),
                        (Ok(_), Some(cleanup)) => format!("The transaction ended; closing its session failed: {cleanup}"),
                    };
                    cx.notify();
                    return;
                }
                transaction.running = false;
                if ran.is_ok() {
                    transaction.statements += 1;
                    transaction.last_activity = Instant::now();
                }
                if this.loads.generation == generation {
                    match ran {
                        Ok(table) => {
                            this.add_run(Some(Arc::new(table)));
                            if !ended {
                                this.status = "Ran in the transaction (not committed yet)".into();
                            }
                        }
                        Err(message) => this.status = format!("The statement failed inside the transaction: {message}"),
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
    }

    pub(crate) fn end_transaction(&mut self, commit: bool, cx: &mut Context<Self>) {
        let Some(transaction) = self.transaction.as_mut() else {
            return;
        };
        if transaction.finishing || transaction.running {
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

    pub(crate) fn transaction_controls(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
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
        div()
            .flex()
            .items_center()
            .gap_2()
            .child({
                let warning = self.transaction_idle_warning();
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
                    .when_some(warning, |badge, warning| {
                        badge.tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(warning.clone()).build(window, cx)
                        })
                    })
                    .child(format!(
                        "Transaction · {}:{:02} · {} run{}",
                        elapsed / 60,
                        elapsed % 60,
                        transaction.statements,
                        if self.transaction_idle_warning().is_some() {
                            " · idle"
                        } else {
                            ""
                        }
                    ))
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
    }
}
