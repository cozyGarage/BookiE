use crate::surfaces::{ButtonKind, ConfirmDialog, button, confirm_dialog};
use crate::*;

impl GridView {
    pub(crate) fn needs_close_confirmation(&self) -> bool {
        self.has_view_edits()
    }

    fn pending_edit_count(&self) -> usize {
        self.cell_edit.edits.len()
            + self.cell_edit.table.len()
            + self.run_tabs.slots.iter().map(|slot| slot.edits.len()).sum::<usize>()
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
        let count = self.pending_edit_count().max(1);
        let dialog = ConfirmDialog {
            id: "close-confirm",
            title: format!(
                "Close the window and discard {}?",
                bookie_gpui::messages::plural(count, "pending change", "pending changes")
            )
            .into(),
            body: "Nothing has been saved to the database. The query text is kept for Reopen closed tab.".into(),
            sql: None,
        };
        let actions = [
            button(ButtonKind::Destructive, "discard-and-close", "Discard and close", theme)
                .debug_selector(|| "discard-and-close".into())
                .on_click(cx.listener(|this, _, window, cx| this.discard_and_close(window, cx))),
            button(ButtonKind::Ghost, "keep-editing", "Keep editing", theme)
                .debug_selector(|| "keep-editing".into())
                .on_click(cx.listener(|this, _, _, cx| this.keep_editing(cx))),
        ];
        Some(confirm_dialog(dialog, actions, theme))
    }
}
