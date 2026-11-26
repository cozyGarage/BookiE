use std::collections::VecDeque;
use std::time::Duration;

use bookie_app_services::approval_prompt::{ApprovalInbox, ApprovalOutcome, PendingApproval, prompt_channel};

use crate::chrome::tip;
use crate::surfaces::{ButtonKind, button, dialog_surface, scrim};
use crate::*;

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
                    if this
                        .approval
                        .current
                        .as_ref()
                        .is_some_and(|open| open.respond.is_closed())
                    {
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

    pub(crate) fn approval_overlay(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let request = &self.approval.current.as_ref()?.request;
        Some(
            scrim(theme).id("approval-overlay").occlude().child(
                dialog_surface(size::APPROVAL_DIALOG_WIDTH, theme)
                    .id("approval-dialog")
                    .debug_selector(|| "approval-dialog".into())
                    .max_w(gpui::relative(0.92))
                    .max_h(gpui::relative(0.9))
                    .gap_2()
                    .p_3()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("This write needs your approval"),
                    )
                    .child(
                        div()
                            .id("approval-detail")
                            .max_h(px(size::SAVE_PREVIEW_MAX_HEIGHT))
                            .overflow_y_scroll()
                            .text_size(px(size::TEXT_SMALL))
                            .text_color(rgb(theme.muted_foreground))
                            .child(approval_detail(request)),
                    )
                    .child(
                        div()
                            .id("approval-sql")
                            .max_h(px(size::SAVE_PREVIEW_MAX_HEIGHT))
                            .overflow_y_scroll()
                            .p_2()
                            .bg(rgb(theme.chrome))
                            .rounded(px(theme.radius))
                            .font_family("monospace")
                            .text_size(px(size::TEXT_SMALL))
                            .child(request.sql.clone()),
                    )
                    .child(approval_actions(theme, cx)),
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
        .justify_end()
        .gap_2()
        .child(
            button(ButtonKind::Primary, "approve-write", "Approve once", theme)
                .debug_selector(|| "approve-write".into())
                .tooltip(tip("Run this statement once"))
                .on_click(cx.listener(|this, _, _, cx| this.answer_approval(ApprovalOutcome::AllowOnce, cx))),
        )
        .child(
            button(ButtonKind::Ghost, "deny-write", "Deny", theme)
                .debug_selector(|| "deny-write".into())
                .on_click(cx.listener(|this, _, _, cx| this.answer_approval(ApprovalOutcome::Deny, cx))),
        )
}
