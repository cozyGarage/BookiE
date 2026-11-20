use std::path::Path;
use std::process::{Command, Stdio};

use super::*;

const LAUNCH_VARIABLES: [&str; 4] = [
    "BOOKIE_PROTOTYPE_FILE",
    "BOOKIE_PROTOTYPE_CONNECTION",
    "BOOKIE_PROTOTYPE_TABLE",
    "BOOKIE_PROTOTYPE_START",
];

pub(crate) fn new_window_command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    for name in LAUNCH_VARIABLES {
        command.env_remove(name);
    }
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    command
}

impl GridView {
    pub(crate) fn new_window_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "new-window",
            label: "Open a new window".to_string(),
            enabled: true,
        }]
    }

    pub(crate) fn disconnect_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "disconnect",
            label: "Disconnect (close this window and open a clean one)".to_string(),
            enabled: self.live.is_some(),
        }]
    }

    pub(crate) fn disconnect(&mut self, cx: &mut Context<Self>) {
        if self.live.is_none() {
            self.status = "There is no connection to disconnect".into();
            return;
        }
        if self.save.busy || self.save.pending.is_some() || !self.cell_edit.edits.is_empty() || self.has_view_edits() {
            self.status = "Save or discard the pending edits before disconnecting".into();
            cx.notify();
            return;
        }
        match std::env::current_exe().and_then(|executable| new_window_command(&executable).spawn()) {
            Ok(_) => cx.quit(),
            Err(error) => {
                self.status = format!("Could not open a clean window, so the connection stays open: {error}");
                cx.notify();
            }
        }
    }

    pub(crate) fn open_new_window(&mut self, cx: &mut Context<Self>) {
        self.status = match std::env::current_exe().and_then(|executable| new_window_command(&executable).spawn()) {
            Ok(_) => "Opened a new window".into(),
            Err(error) => format!("Could not open a new window: {error}"),
        };
        cx.notify();
    }
}

#[cfg(test)]
#[path = "new_window_ui_tests.rs"]
mod tests;
