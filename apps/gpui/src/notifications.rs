use gpui::{Context, Div, Stateful, div, prelude::*, px, rgb};

use crate::GridView;
use crate::surfaces::{ButtonKind, ListState, compact_button, compact_icon_button, list_state, section_header};
use bookie_gpui::theme::{Theme, size};

const KEEP: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoticeKind {
    Success,
    Error,
    UnknownOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Notice {
    pub id: u64,
    pub text: String,
    pub kind: NoticeKind,
    pub read: bool,
}

#[derive(Default)]
pub(crate) struct Notices {
    pub items: Vec<Notice>,
    pub open: bool,
    next_id: u64,
}

impl Notices {
    pub fn push(&mut self, text: String, kind: NoticeKind) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.items.insert(
            0,
            Notice {
                id,
                text,
                kind,
                read: kind == NoticeKind::Success,
            },
        );
        while self.items.len() > KEEP {
            let oldest_read = self.items.iter().rposition(|notice| notice.read);
            match oldest_read {
                Some(index) => self.items.remove(index),
                None => self.items.remove(self.items.len() - 1),
            };
        }
        id
    }

    pub fn unread(&self) -> usize {
        self.items.iter().filter(|notice| !notice.read).count()
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|notice| notice.id != id);
    }

    pub fn clear_read(&mut self) {
        self.items.retain(|notice| !notice.read);
    }
}

impl GridView {
    pub(crate) fn push_notice(&mut self, text: String, kind: NoticeKind, cx: &mut Context<Self>) {
        self.notices.push(text, kind);
        cx.notify();
    }

    pub(crate) fn toggle_notifications(&mut self, cx: &mut Context<Self>) {
        self.notices.open = !self.notices.open;
        cx.notify();
    }

    pub(crate) fn dismiss_notice(&mut self, id: u64, cx: &mut Context<Self>) {
        self.notices.dismiss(id);
        cx.notify();
    }

    pub(crate) fn notifications_indicator(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let unread = self.notices.unread();
        let tooltip = match unread {
            0 => "Notifications".to_string(),
            1 => "Notifications: 1 needs attention".to_string(),
            n => format!("Notifications: {n} need attention"),
        };
        let colour = if unread > 0 {
            theme.destructive
        } else {
            theme.muted_foreground
        };
        compact_icon_button("notifications", "bell", tooltip, theme)
            .debug_selector(|| "notifications".into())
            .flex_none()
            .when(unread > 0, |button| {
                button.child(
                    div()
                        .debug_selector(|| "notifications-count".into())
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(colour))
                        .child(unread.to_string()),
                )
            })
            .on_click(cx.listener(|this, _, _, cx| this.toggle_notifications(cx)))
    }

    pub(crate) fn notifications_list(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !self.notices.open {
            return None;
        }
        let rows: Vec<Div> = self
            .notices
            .items
            .iter()
            .map(|notice| self.notice_row(notice, theme, cx))
            .collect();
        let empty = rows
            .is_empty()
            .then(|| list_state(ListState::Empty("No notifications"), theme));
        Some(
            div()
                .id("notifications-anchor")
                .absolute()
                .right(px(size::SM))
                .bottom(px(crate::chrome::STATUS_HEIGHT + size::XS))
                .child(
                    crate::surfaces::dialog_surface(size::NOTIFICATIONS_WIDTH, theme)
                        .id("notifications-list")
                        .debug_selector(|| "notifications-list".into())
                        .max_h(px(size::NOTIFICATIONS_MAX_HEIGHT))
                        .occlude()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_3()
                                .py_2()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .child(div().flex_1().child(section_header("Notifications", theme)))
                                .children(self.notices.items.iter().any(|notice| notice.read).then(|| {
                                    compact_button(ButtonKind::Ghost, "clear-read-notices", "Clear read", theme)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.notices.clear_read();
                                            cx.notify();
                                        }))
                                }))
                                .child(
                                    compact_icon_button("close-notifications", "x", "Close", theme)
                                        .on_click(cx.listener(|this, _, _, cx| this.toggle_notifications(cx))),
                                ),
                        )
                        .child(
                            div()
                                .id("notifications-rows")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .children(rows)
                                .children(empty),
                        ),
                ),
        )
    }

    fn notice_row(&self, notice: &Notice, theme: Theme, cx: &mut Context<Self>) -> Div {
        let id = notice.id;
        let colour = match (notice.kind, notice.read) {
            (NoticeKind::Success, _) => theme.muted_foreground,
            (_, false) => theme.destructive,
            (_, true) => theme.foreground,
        };
        let resolve = (notice.kind == NoticeKind::UnknownOutcome).then(|| {
            compact_button(
                ButtonKind::Ghost,
                ("resolve-notice", id as usize),
                "Resolve outcome…",
                theme,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.notices.open = false;
                this.open_resolve_panel(cx);
                cx.notify();
            }))
        });
        div()
            .debug_selector(move || format!("notice-{id}"))
            .flex()
            .items_start()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(rgb(colour))
                    .child(notice.text.clone()),
            )
            .children(resolve)
            .child(
                compact_icon_button(("dismiss-notice", id as usize), "x", "Dismiss", theme)
                    .debug_selector(move || format!("dismiss-notice-{id}"))
                    .on_click(cx.listener(move |this, _, _, cx| this.dismiss_notice(id, cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{KEEP, NoticeKind, Notices};

    #[test]
    fn a_success_arrives_read_and_a_failure_waits_until_dismissed() {
        let mut notices = Notices::default();
        notices.push("Saved 2 change(s)".into(), NoticeKind::Success);
        let failed = notices.push("Could not save".into(), NoticeKind::Error);
        let unknown = notices.push(
            "The save may or may not have been applied".into(),
            NoticeKind::UnknownOutcome,
        );
        assert_eq!(notices.unread(), 2);
        assert_eq!(notices.items[0].id, unknown, "the newest comes first");
        notices.clear_read();
        assert_eq!(notices.items.len(), 2, "clearing read notices keeps the failures");
        notices.dismiss(failed);
        notices.dismiss(unknown);
        assert_eq!(notices.unread(), 0);
        assert!(notices.items.is_empty());
    }

    #[test]
    fn the_list_keeps_unread_failures_when_it_is_full() {
        let mut notices = Notices::default();
        let failed = notices.push("Could not save".into(), NoticeKind::Error);
        for n in 0..KEEP + 10 {
            notices.push(format!("Saved {n}"), NoticeKind::Success);
        }
        assert_eq!(notices.items.len(), KEEP);
        assert!(notices.items.iter().any(|notice| notice.id == failed));
    }
}
