use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bookie_app_services::read_session_store::SubscriptionMessage;
use tokio_util::sync::{CancellationToken, DropGuard};

use super::*;

const FOLLOW_BUFFER: usize = 256;

pub(crate) struct FollowState {
    pub channel: String,
    pub id: u64,
    pub received: u64,
    reported_dropped: u64,
    dropped: Arc<AtomicU64>,
    cancel: CancellationToken,
    _stop_on_drop: DropGuard,
}

impl FollowState {
    fn new(channel: String, id: u64, dropped: Arc<AtomicU64>) -> Self {
        let cancel = CancellationToken::new();
        Self {
            channel,
            id,
            received: 0,
            reported_dropped: 0,
            dropped,
            _stop_on_drop: cancel.clone().drop_guard(),
            cancel,
        }
    }
}

pub(crate) enum FollowEvent {
    Message(u64, SubscriptionMessage),
    Ended(u64, Result<u64, String>),
}

impl GridView {
    pub(crate) fn start_follow(&mut self, channel: String, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else {
            return;
        };
        if let Some(active) = &self.panel.follow {
            self.status = format!("Already following {}; stop that first", active.channel);
            return;
        }
        self.panel.follow_serial += 1;
        let id = self.panel.follow_serial;
        let dropped = Arc::new(AtomicU64::new(0));
        let state = FollowState::new(channel.clone(), id, dropped.clone());
        let cancel = state.cancel.clone();
        self.panel.follow = Some(state);
        self.status = format!("Following {channel} until you stop it…");
        let (tx, mut inbox) = tokio::sync::mpsc::channel::<FollowEvent>(FOLLOW_BUFFER);
        let worker_tx = tx.clone();
        cx.background_executor()
            .spawn(async move {
                let on_message_tx = worker_tx.clone();
                let result = live.follow_pubsub(
                    &channel,
                    &cancel,
                    Box::new(move |message| {
                        if on_message_tx.try_send(FollowEvent::Message(id, message)).is_err() {
                            dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    }),
                );
                let _ = worker_tx.send(FollowEvent::Ended(id, result)).await;
            })
            .detach();
        drop(tx);
        cx.spawn(async move |this, cx| {
            while let Some(event) = inbox.recv().await {
                if this.update(cx, |this, cx| this.apply_follow_event(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn stop_follow(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = &self.panel.follow {
            state.cancel.cancel();
            self.status = format!("Stopping {}…", state.channel);
            cx.notify();
        }
    }

    pub(crate) fn apply_follow_event(&mut self, event: FollowEvent, cx: &mut Context<Self>) {
        match event {
            FollowEvent::Message(id, message) => {
                let Some(state) = self.panel.follow.as_mut().filter(|state| state.id == id) else {
                    return;
                };
                state.received += 1;
                let missed = state.dropped.load(Ordering::Relaxed);
                let newly_dropped = missed - state.reported_dropped;
                state.reported_dropped = missed;
                self.status = format!("Following {} · {} messages", state.channel, state.received);
                if newly_dropped > 0 {
                    self.panel.messages.push(
                        MessageKind::Info,
                        format!("{newly_dropped} notifications were dropped because the window could not keep up"),
                    );
                }
                self.panel
                    .messages
                    .push(MessageKind::Info, bookie_gpui::pubsub_view::message_line(&message));
            }
            FollowEvent::Ended(id, result) => {
                let Some(state) = self.panel.follow.take_if(|state| state.id == id) else {
                    return;
                };
                let (kind, text) = match result {
                    Ok(count) => (
                        MessageKind::Success,
                        format!("Stopped following {} · {count} messages", state.channel),
                    ),
                    Err(error) => (
                        MessageKind::Error,
                        format!("Following {} ended: {error}", state.channel),
                    ),
                };
                self.status = text.clone();
                self.panel.messages.push(kind, text);
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
pub(crate) fn follow_state_for_test(channel: &str, id: u64) -> FollowState {
    FollowState::new(channel.into(), id, Arc::new(AtomicU64::new(0)))
}
