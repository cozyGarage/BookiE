use std::collections::VecDeque;
use std::time::Duration;

use bookie_app_services::approval_prompt::{ApprovalInbox, ApprovalOutcome, PendingApproval, prompt_channel};

use super::*;
use crate::chrome::{ghost, primary_button};

pub(crate) struct ApprovalUi {
    pub tx: tokio::sync::mpsc::UnboundedSender<PendingApproval>,
    pub current: Option<PendingApproval>,
    queue: VecDeque<PendingApproval>,
    serial: u64,
}

impl ApprovalUi {
    pub fn new() -> (Self, ApprovalInbox) {
        let (tx, inbox) = prompt_channel();
        let ui = Self {
            tx,
            current: None,
            queue: VecDeque::new(),
            serial: 0,
        };
        (ui, inbox)
    }
}

impl GridView {
    pub(crate) fn watch_approvals(inbox: ApprovalInbox, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut inbox = inbox;
            while let Some(pending) = inbox.recv().await {
                if this.update(cx, |this, cx| this.show_approval(pending, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn show_approval(&mut self, pending: PendingApproval, cx: &mut Context<Self>) {
        if self.approval.current.is_some() {
            self.approval.queue.push_back(pending);
        } else {
            self.activate_approval(pending, cx);
        }
        cx.notify();
    }

    fn activate_approval(&mut self, pending: PendingApproval, cx: &mut Context<Self>) {
        self.approval.serial += 1;
        let serial = self.approval.serial;
        self.approval.current = Some(pending);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let done = this.update(cx, |this, cx| {
                    if this.approval.serial != serial || this.approval.current.is_none() {
                        return true;
                    }
                    if this.approval.current.as_ref().is_some_and(|open| open.respond.is_closed()) {
                        this.next_approval(cx);
                        return true;
                    }
                    false
                });
                if done.unwrap_or(true) {
                    break;
                }
            }
        })
        .detach();
    }

    fn next_approval(&mut self, cx: &mut Context<Self>) {
        self.approval.current = None;
        while let Some(next) = self.approval.queue.pop_front() {
            if !next.respond.is_closed() {
                self.activate_approval(next, cx);
                break;
            }
        }
        cx.notify();
    }

    pub(crate) fn answer_approval(&mut self, outcome: ApprovalOutcome, cx: &mut Context<Self>) {
        if let Some(open) = self.approval.current.take() {
            let _ = open.respond.send(outcome);
        }
        self.next_approval(cx);
    }

    pub(crate) fn approval_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let request = &self.approval.current.as_ref()?.request;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.destructive))
                .child(approval_actions(theme, cx))
                .child(
                    div()
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child(approval_detail(request)),
                )
                .child(
                    div()
                        .id("approval-sql")
                        .max_h(px(size::SAVE_PREVIEW_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .font_family("monospace")
                        .text_size(px(size::TEXT_SMALL))
                        .child(request.sql.clone()),
                ),
        )
    }
}

fn approval_detail(request: &bookie_app_services::approval_prompt::ApprovalRequest) -> String {
    let mut detail = vec![
        format!("Rule: {}", request.rule),
        request.reason.clone(),
        format!(
            "Connection: {} ({})  Class: {:?}",
            request.connection_name,
            request.environment.display_name(),
            request.facts.class
        ),
    ];
    if let Some(rows) = request.estimated_rows {
        detail.push(format!("Exact affected-row count: {rows}"));
    }
    if let Some(preview) = &request.preview {
        detail.push(preview.clone());
    }
    detail.join("\n")
}

fn approval_actions(theme: Theme, cx: &mut Context<GridView>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child("This write needs your approval"),
        )
        .child(
            ghost("deny-write", "Deny", theme)
                .on_click(cx.listener(|this, _, _, cx| this.answer_approval(ApprovalOutcome::Deny, cx))),
        )
        .child(
            primary_button("approve-write", "Approve once", "Run this statement once", theme)
                .on_click(cx.listener(|this, _, _, cx| this.answer_approval(ApprovalOutcome::AllowOnce, cx))),
        )
}
