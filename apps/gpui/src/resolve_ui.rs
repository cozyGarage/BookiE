use bookie_app_services::unknown_outcomes::{
    OutcomeDecision, UnknownOutcome, list_unknown_outcomes, resolve_unknown_outcome,
};

use super::*;
use crate::chrome::ghost;

#[derive(Default)]
pub(crate) struct ResolveState {
    pub open: bool,
    pub busy: bool,
    pub items: Vec<UnknownOutcome>,
    pub reason: Option<Entity<TextInput>>,
}

fn blocking<T: Send + 'static>(
    future: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
) -> Result<T, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?
        .block_on(future)
}

impl GridView {
    pub(crate) fn resolve_enabled(&self) -> bool {
        self.live.as_ref().is_some_and(|live| live.writable.is_some()) && !self.resolve.open
    }

    pub(crate) fn resolve_panel_is_closed(&self) -> bool {
        !self.resolve.open && self.resolve.items.is_empty()
    }

    pub(crate) fn close_resolve_panel(&mut self) {
        self.resolve.open = false;
        self.resolve.items.clear();
    }

    pub(crate) fn open_resolve_panel(&mut self, cx: &mut Context<Self>) {
        let Some(connection_id) = self
            .live
            .as_ref()
            .and_then(|live| live.saved.as_ref().map(|(id, _)| *id))
        else {
            return;
        };
        if !self.resolve_enabled() {
            return;
        }
        self.status = "Reading the audit journal…".into();
        cx.spawn(async move |this, cx| {
            let listed = cx
                .background_executor()
                .spawn(async move { blocking(async move { list_unknown_outcomes(Some(connection_id)).await }) })
                .await;
            this.update(cx, |this, cx| {
                match listed {
                    Ok(items) if items.is_empty() => this.status = "No write outcome is waiting to be resolved".into(),
                    Ok(items) => {
                        let reason = this.resolve.reason.get_or_insert_with(|| {
                            cx.new(|cx| TextInput::new("Why you decided this (required, no secrets)", cx))
                        });
                        reason.update(cx, |input, cx| input.set_content("", cx));
                        this.resolve.items = items;
                        this.resolve.open = true;
                        this.status = "Decide what happened to the write below".into();
                    }
                    Err(message) => this.status = format!("Could not read the audit journal: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn decide_outcome(&mut self, decision: OutcomeDecision, cx: &mut Context<Self>) {
        let (Some(item), Some(reason)) = (self.resolve.items.first().cloned(), self.resolve.reason.clone()) else {
            return;
        };
        if self.resolve.busy {
            return;
        }
        let text = reason.read(cx).content().trim().to_string();
        if text.is_empty() {
            self.status = "Say why you decided this; the reason is recorded in the audit journal".into();
            return;
        }
        self.resolve.busy = true;
        let connection_id = item.connection_id;
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    blocking(async move {
                        resolve_unknown_outcome(item.operation_id, decision, &text).await?;
                        list_unknown_outcomes(Some(connection_id)).await
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.resolve.busy = false;
                match outcome {
                    Ok(left) => {
                        this.save.write = None;
                        this.status = match decision {
                            OutcomeDecision::StillUnknown => "Recorded: still unknown. Writes stay blocked".into(),
                            _ => "Decision recorded. The connection accepts writes again once none is left".into(),
                        };
                        this.resolve.items = left;
                        this.resolve.open = !this.resolve.items.is_empty();
                        if let Some(reason) = &this.resolve.reason {
                            reason.update(cx, |input, cx| input.set_content("", cx));
                        }
                    }
                    Err(message) => this.status = format!("Could not record the decision: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn resolve_choices(&self, reason: Entity<TextInput>, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(div().flex_1().min_w_0().child(reason))
            .child(
                ghost("resolve-committed", "It was applied", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.decide_outcome(OutcomeDecision::Committed, cx))),
            )
            .child(
                ghost("resolve-rolled-back", "It was not applied", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.decide_outcome(OutcomeDecision::RolledBack, cx))),
            )
            .child(
                ghost("resolve-unknown", "Still unknown", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.decide_outcome(OutcomeDecision::StillUnknown, cx))),
            )
    }

    pub(crate) fn resolve_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let item = self.resolve.items.first().filter(|_| self.resolve.open)?;
        let reason = self.resolve.reason.clone()?;
        let record = record_text(item);
        let heading = format!("A write's result is unknown ({} waiting)", self.resolve.items.len());
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(div().font_weight(gpui::FontWeight::MEDIUM).flex_1().child(heading))
                        .child(
                            ghost("close-resolve", "Close", theme).on_click(cx.listener(|this, _, _, cx| {
                                this.close_resolve_panel();
                                cx.notify();
                            })),
                        ),
                )
                .child(
                    div()
                        .id("resolve-record")
                        .max_h(px(size::SAVE_PREVIEW_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .font_family("monospace")
                        .text_size(px(size::TEXT_SMALL))
                        .child(record),
                )
                .child(
                    div()
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child(
                            "Check the database yourself first. Your choice is recorded as your claim, not proof; \
                             BookiE will not replay or re-check the write. Do not put secrets in the reason.",
                        ),
                )
                .child(self.resolve_choices(reason, theme, cx)),
        )
    }
}

fn record_text(item: &UnknownOutcome) -> String {
    let error = item
        .error
        .as_ref()
        .map(|error| format!("\n{error}"))
        .unwrap_or_default();
    format!(
        "{} · {} · {} · {}{error}\n{}",
        item.connection_name, item.environment, item.class, item.when, item.sql
    )
}
