use std::collections::VecDeque;
use std::time::Duration;

use bookie_app_services::ssh_prompt::{HostKeyInbox, PendingHostKey, host_key_channel};

use super::*;
use crate::chrome::{ghost, primary_button};

pub(crate) struct HostKeyUi {
    pub tx: tokio::sync::mpsc::UnboundedSender<PendingHostKey>,
    pub current: Option<PendingHostKey>,
    queue: VecDeque<PendingHostKey>,
    serial: u64,
}

impl HostKeyUi {
    pub fn new() -> (Self, HostKeyInbox) {
        let (tx, inbox) = host_key_channel();
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
    pub(crate) fn watch_host_keys(inbox: HostKeyInbox, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut inbox = inbox;
            while let Some(pending) = inbox.recv().await {
                if this.update(cx, |this, cx| this.show_host_key(pending, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn show_host_key(&mut self, pending: PendingHostKey, cx: &mut Context<Self>) {
        if self.host_key.current.is_some() {
            self.host_key.queue.push_back(pending);
        } else {
            self.activate_host_key(pending, cx);
        }
        cx.notify();
    }

    fn activate_host_key(&mut self, pending: PendingHostKey, cx: &mut Context<Self>) {
        self.host_key.serial += 1;
        let serial = self.host_key.serial;
        self.host_key.current = Some(pending);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let done = this.update(cx, |this, cx| {
                    let ui = &this.host_key;
                    if ui.serial != serial || ui.current.is_none() {
                        return true;
                    }
                    if ui.current.as_ref().is_some_and(|open| open.respond.is_closed()) {
                        this.next_host_key(cx);
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

    fn next_host_key(&mut self, cx: &mut Context<Self>) {
        self.host_key.current = None;
        while let Some(next) = self.host_key.queue.pop_front() {
            if !next.respond.is_closed() {
                self.activate_host_key(next, cx);
                break;
            }
        }
        cx.notify();
    }

    pub(crate) fn answer_host_key(&mut self, trust: bool, cx: &mut Context<Self>) {
        if let Some(open) = self.host_key.current.take() {
            let _ = open.respond.send(trust);
        }
        self.next_host_key(cx);
    }

    pub(crate) fn host_key_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let open = self.host_key.current.as_ref()?;
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
                .child(host_key_actions(theme, cx))
                .child(
                    div()
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child(host_key_detail(open)),
                ),
        )
    }
}

pub(crate) fn host_key_detail(open: &PendingHostKey) -> String {
    format!(
        "This computer has not connected to {} before. Its {} key fingerprint is {}. Compare it with the fingerprint your server administrator gave you before trusting it.",
        open.host, open.algorithm, open.fingerprint
    )
}

fn host_key_actions(theme: Theme, cx: &mut Context<GridView>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child("Trust this SSH host?"),
        )
        .child(
            ghost("refuse-host-key", "Cancel", theme)
                .debug_selector(|| "refuse-host-key".into())
                .on_click(cx.listener(|this, _, _, cx| this.answer_host_key(false, cx))),
        )
        .child(
            primary_button("trust-host-key", "Trust", "Record this host key and connect", theme)
                .debug_selector(|| "trust-host-key".into())
                .on_click(cx.listener(|this, _, _, cx| this.answer_host_key(true, cx))),
        )
}
