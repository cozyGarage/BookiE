use bookie_app_services::service_runtime::block_on;
use bookie_app_services::unknown_outcomes::{
    OutcomeDecision, UnknownOutcome, UnknownOutcomeError, list_unknown_outcomes, resolve_unknown_outcome,
};

use crate::surfaces::{ButtonKind, button, task_bar_frame, task_title};
use crate::*;

#[derive(Default)]
pub(crate) struct ResolveState {
    pub open: bool,
    pub busy: bool,
    pub items: Vec<UnknownOutcome>,
    pub choice: Option<OutcomeDecision>,
    pub checked: bool,
    pub reason: Option<Entity<TextInput>>,
}

pub(crate) fn decision_status(decided: &Result<(), UnknownOutcomeError>, decision: OutcomeDecision) -> String {
    match decided {
        Ok(()) if decision == OutcomeDecision::StillUnknown => "Recorded: still unknown. Writes stay blocked".into(),
        Ok(()) => "Decision recorded. The connection accepts writes again once none is left".into(),
        Err(UnknownOutcomeError::NoLongerUnresolved) => {
            "That write was already decided elsewhere; the list was reloaded".into()
        }
        Err(error) => format!("Could not record the decision: {error}"),
    }
}

impl GridView {
    pub(crate) fn resolve_enabled(&self) -> bool {
        self.live.as_ref().is_some_and(|live| live.writable.is_some()) && !self.resolve.open
    }

    #[cfg(test)]
    pub(crate) fn resolve_panel_is_closed(&self) -> bool {
        !self.resolve.open && self.resolve.items.is_empty()
    }

    pub(crate) fn close_resolve_panel(&mut self) {
        self.resolve.open = false;
        self.resolve.items.clear();
        self.resolve.choice = None;
        self.resolve.checked = false;
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
                .spawn(async move {
                    block_on(async move {
                        list_unknown_outcomes(Some(connection_id))
                            .await
                            .map_err(|error| error.to_string())
                    })
                })
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
        let checked = self.resolve.checked;
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    block_on(async move {
                        let decided = resolve_unknown_outcome(item.operation_id, decision, &text, checked).await;
                        let left = list_unknown_outcomes(Some(connection_id))
                            .await
                            .map_err(|error| error.to_string())?;
                        Ok::<_, String>((decided, left))
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.resolve.busy = false;
                this.resolve.choice = None;
                this.resolve.checked = false;
                match outcome {
                    Ok((decided, left)) => {
                        let applied = decided.is_ok();
                        this.status = decision_status(&decided, decision);
                        if applied {
                            this.save.write = None;
                            if let Some(reason) = &this.resolve.reason {
                                reason.update(cx, |input, cx| input.set_content("", cx));
                            }
                        }
                        this.resolve.items = left;
                        this.resolve.open = !this.resolve.items.is_empty();
                    }
                    Err(message) => this.status = format!("Could not read the audit journal: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn choose_outcome(&mut self, decision: OutcomeDecision, cx: &mut Context<Self>) {
        let reason_given = self
            .resolve
            .reason
            .as_ref()
            .is_some_and(|reason| !reason.read(cx).content().trim().is_empty());
        if !reason_given {
            self.status = "Say why you decided this; the reason is recorded in the audit journal".into();
            return;
        }
        self.resolve.choice = Some(decision);
        self.resolve.checked = false;
        cx.notify();
    }

    fn resolve_confirm(
        &self,
        decision: OutcomeDecision,
        item: &UnknownOutcome,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let claim = match decision {
            OutcomeDecision::Committed => "WAS applied",
            _ => "was NOT applied",
        };
        let where_ = if item.connection_id.is_nil() {
            "every connection".to_string()
        } else {
            format!("{} ({})", item.connection_name, item.environment)
        };
        let mark = if self.resolve.checked { "[x]" } else { "[ ]" };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().child(format!(
                "You are recording that the write {claim}. This lifts the write block on {where_} and cannot be undone."
            )))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        button(
                            ButtonKind::Ghost,
                            "resolve-checked",
                            format!("{mark} I checked the database myself"),
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.resolve.checked = !this.resolve.checked;
                            cx.notify();
                        })),
                    )
                    .child(
                        button(ButtonKind::Primary, "resolve-confirm", "Confirm", theme)
                            .on_click(cx.listener(move |this, _, _, cx| this.decide_outcome(decision, cx))),
                    )
                    .child(
                        button(ButtonKind::Ghost, "resolve-back", "Back", theme).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.resolve.choice = None;
                                cx.notify();
                            },
                        )),
                    ),
            )
    }

    fn resolve_choices(&self, reason: Entity<TextInput>, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(div().flex_1().min_w_0().child(reason))
            .child(
                button(ButtonKind::Ghost, "resolve-committed", "It was applied", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_outcome(OutcomeDecision::Committed, cx))),
            )
            .child(
                button(ButtonKind::Ghost, "resolve-rolled-back", "It was not applied", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_outcome(OutcomeDecision::RolledBack, cx))),
            )
            .child(
                button(ButtonKind::Ghost, "resolve-unknown", "Still unknown", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.decide_outcome(OutcomeDecision::StillUnknown, cx))),
            )
    }

    pub(crate) fn resolve_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let item = self.resolve.items.first().filter(|_| self.resolve.open)?;
        let reason = self.resolve.reason.clone()?;
        let record = record_text(item);
        let scope = if item.connection_id.is_nil() {
            "; blocks every connection"
        } else {
            ""
        };
        let heading = format!(
            "A write's result is unknown ({} waiting{scope})",
            self.resolve.items.len()
        );
        Some(
            task_bar_frame(theme)
                .flex_col()
                .items_stretch()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(task_title(heading).flex_1())
                        .child(
                            button(ButtonKind::Ghost, "close-resolve", "Close", theme).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.close_resolve_panel();
                                    cx.notify();
                                },
                            )),
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
                .child(match self.resolve.choice {
                    Some(decision) => self.resolve_confirm(decision, item, theme, cx),
                    None => self.resolve_choices(reason, theme, cx),
                }),
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
