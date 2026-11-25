use super::*;
use crate::surfaces::{ButtonKind, button, dialog_surface, scrim};

impl GridView {
    pub(crate) fn needs_close_confirmation(&self) -> bool {
        !self.cell_edit.edits.is_empty()
    }

    pub(crate) fn keep_editing(&mut self, cx: &mut Context<Self>) {
        if self.close_confirm {
            self.close_confirm = false;
            self.status = "Kept the window open".into();
        }
        cx.notify();
    }

    pub(crate) fn discard_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_confirm = false;
        self.remember_open_query_draft(cx);
        window.remove_window();
    }

    pub(crate) fn close_confirm_overlay(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !self.close_confirm {
            return None;
        }
        let count = self.cell_edit.edits.len();
        Some(
            scrim(theme).id("close-confirm-overlay").occlude().child(
                dialog_surface(size::APPROVAL_DIALOG_WIDTH, theme)
                    .id("close-confirm-dialog")
                    .debug_selector(|| "close-confirm-dialog".into())
                    .max_w(gpui::relative(0.92))
                    .gap_2()
                    .p_3()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(format!("Close the window and discard {count} pending change(s)?")),
                    )
                    .child(
                        div()
                            .text_size(px(size::TEXT_SMALL))
                            .text_color(rgb(theme.muted_foreground))
                            .child(
                                "Nothing has been saved to the database. The query text is kept for Reopen closed tab.",
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                button(ButtonKind::Destructive, "discard-and-close", "Discard and close", theme)
                                    .debug_selector(|| "discard-and-close".into())
                                    .on_click(cx.listener(|this, _, window, cx| this.discard_and_close(window, cx))),
                            )
                            .child(
                                button(ButtonKind::Ghost, "keep-editing", "Keep editing", theme)
                                    .debug_selector(|| "keep-editing".into())
                                    .on_click(cx.listener(|this, _, _, cx| this.keep_editing(cx))),
                            ),
                    ),
            ),
        )
    }
}
