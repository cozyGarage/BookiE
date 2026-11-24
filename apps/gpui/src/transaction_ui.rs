use std::sync::Arc;
use std::time::Instant;

use bookie_app_services::interactive_session::SharedSession;
use bookie_gpui::live::WriteSession;

use super::*;
use crate::chrome::ghost;

pub(crate) struct TransactionState {
    pub write: Arc<WriteSession>,
    pub shared: SharedSession,
    pub started: Instant,
    pub statements: usize,
}

impl GridView {
    pub(crate) fn transaction_open(&self) -> bool {
        self.transaction.is_some()
    }

    pub(crate) fn begin_transaction(&mut self, cx: &mut Context<Self>) {
        if self.transaction.is_some() {
            return;
        }
        let Some((connection_id, opened_with)) = self.live.as_ref().and_then(|live| {
            live.saved
                .as_ref()
                .map(|(id, _)| (*id, live.stamp.clone()))
        }) else {
            self.status = "Transactions need a saved connection".into();
            return;
        };
        let approvals = self.approval.tx.clone();
        self.status = "Starting a transaction…".into();
        self.begin_background(
            Work::Load,
            cx,
            move |_| {
                let write = Arc::new(WriteSession::open_saved(connection_id, approvals, opened_with.as_deref())?);
                let shared = write.begin_transaction()?;
                Ok::<_, String>((write, shared))
            },
            |this, opened| match opened {
                Ok((write, shared)) => {
                    this.transaction = Some(TransactionState {
                        write,
                        shared,
                        started: Instant::now(),
                        statements: 0,
                    });
                    this.status = "Transaction open: statements run in it until you commit or roll back".into();
                }
                Err(message) => this.status = format!("Could not start a transaction: {message}"),
            },
        );
    }

    pub(crate) fn run_in_transaction(&mut self, sql: String, cx: &mut Context<Self>) {
        let (Some(transaction), Some(live)) = (self.transaction.as_ref(), self.live.clone()) else {
            return;
        };
        let (write, shared) = (transaction.write.clone(), transaction.shared.clone());
        self.status = "Running in the transaction…".into();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                let result = write.run_in_transaction(&shared, &sql, &cancel)?;
                live.table_from_result(result, &sql, &cancel)
            },
            |this, ran| match ran {
                Ok(table) => {
                    if let Some(transaction) = this.transaction.as_mut() {
                        transaction.statements += 1;
                    }
                    this.add_run(Some(Arc::new(table)));
                    this.status = "Ran in the transaction (not committed yet)".into();
                }
                Err(message) => this.status = format!("The statement failed inside the transaction: {message}"),
            },
        );
    }

    pub(crate) fn end_transaction(&mut self, commit: bool, cx: &mut Context<Self>) {
        let Some(transaction) = self.transaction.take() else {
            return;
        };
        self.status = if commit { "Committing…" } else { "Rolling back…" }.into();
        self.begin_background(
            Work::Load,
            cx,
            move |_| {
                if commit {
                    transaction.write.commit_transaction(&transaction.shared)
                } else {
                    transaction.write.rollback_transaction(&transaction.shared)
                }
            },
            move |this, ended| {
                this.status = match (ended, commit) {
                    (Ok(()), true) => "Transaction committed".into(),
                    (Ok(()), false) => "Transaction rolled back".into(),
                    (Err(message), true) => format!("Commit failed: {message}"),
                    (Err(message), false) => format!("Roll back failed: {message}"),
                };
            },
        );
    }

    pub(crate) fn transaction_controls(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let Some(transaction) = self.transaction.as_ref() else {
            return div().child(ghost("begin-transaction", "Begin transaction", theme).on_click(cx.listener(
                |this, _, _, cx| {
                    this.begin_transaction(cx);
                    cx.notify();
                },
            )));
        };
        let elapsed = transaction.started.elapsed().as_secs();
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .debug_selector(|| "transaction-badge".into())
                    .px_2()
                    .rounded(px(theme.radius))
                    .border_1()
                    .border_color(rgb(theme.warning))
                    .text_color(rgb(theme.warning))
                    .child(format!(
                        "Transaction · {}:{:02} · {} run",
                        elapsed / 60,
                        elapsed % 60,
                        transaction.statements
                    )),
            )
            .child(ghost("commit-transaction", "Commit", theme).on_click(cx.listener(|this, _, _, cx| {
                this.end_transaction(true, cx);
                cx.notify();
            })))
            .child(ghost("rollback-transaction", "Roll back", theme).on_click(cx.listener(|this, _, _, cx| {
                this.end_transaction(false, cx);
                cx.notify();
            })))
    }
}
