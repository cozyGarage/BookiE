use crate::surfaces::{ButtonKind, ConfirmDialog, button, confirm_dialog};
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
        let dialog = ConfirmDialog {
            id: "cancel-process",
            title: format!("Cancel server process {pid}?").into(),
            body: "The server stops what this process is running. A Prod connection asks for approval first.".into(),
            sql: Some(sql.clone().into()),
        };
        let actions = [
            button(
                ButtonKind::Destructive,
                "confirm-cancel-process",
                "Cancel process",
                theme,
            )
            .debug_selector(|| "confirm-cancel-process".into())
            .on_click(cx.listener(|this, _, _, cx| this.confirm_cancel_process(cx))),
            button(ButtonKind::Ghost, "keep-process", "Keep running", theme)
                .debug_selector(|| "keep-process".into())
                .on_click(cx.listener(|this, _, _, cx| this.keep_process_running(cx))),
        ];
        Some(confirm_dialog(dialog, actions, theme))
    }
}
