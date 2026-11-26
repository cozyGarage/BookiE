use crate::surfaces::{ButtonKind, button, dialog_surface, scrim};
use crate::*;

impl GridView {
    pub(crate) fn ask_to_cancel_process(&mut self, pid: i64, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_ref() else {
            self.status = "No active database connection".into();
            return;
        };
        let Some(sql) = bookie_app_services::server_process::cancel_statement(&live.driver_id, pid) else {
            self.status = "Canceling processes is not supported for this database engine".into();
            return;
        };
        self.cancel_confirm = Some((pid, sql));
        cx.notify();
    }

    pub(crate) fn confirm_cancel_process(&mut self, cx: &mut Context<Self>) {
        if let Some((pid, _)) = self.cancel_confirm.take() {
            self.cancel_server_process(pid, cx);
        }
        cx.notify();
    }

    pub(crate) fn keep_process_running(&mut self, cx: &mut Context<Self>) {
        if self.cancel_confirm.take().is_some() {
            self.status = "Left the server process running".into();
        }
        cx.notify();
    }

    pub(crate) fn cancel_process_overlay(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let (pid, sql) = self.cancel_confirm.as_ref()?;
        Some(
            scrim(theme).id("cancel-process-overlay").occlude().child(
                dialog_surface(size::APPROVAL_DIALOG_WIDTH, theme)
                    .id("cancel-process-dialog")
                    .debug_selector(|| "cancel-process-dialog".into())
                    .max_w(gpui::relative(0.92))
                    .gap_2()
                    .p_3()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(format!("Cancel server process {pid}?")),
                    )
                    .child(
                        div()
                            .text_size(px(size::TEXT_SMALL))
                            .text_color(rgb(theme.muted_foreground))
                            .child("The server stops what this process is running. A Prod connection asks for approval first."),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(size::TEXT_SMALL))
                            .child(sql.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                button(ButtonKind::Destructive, "confirm-cancel-process", "Cancel process", theme)
                                    .debug_selector(|| "confirm-cancel-process".into())
                                    .on_click(cx.listener(|this, _, _, cx| this.confirm_cancel_process(cx))),
                            )
                            .child(
                                button(ButtonKind::Ghost, "keep-process", "Keep running", theme)
                                    .debug_selector(|| "keep-process".into())
                                    .on_click(cx.listener(|this, _, _, cx| this.keep_process_running(cx))),
                            ),
                    ),
            ),
        )
    }
}
