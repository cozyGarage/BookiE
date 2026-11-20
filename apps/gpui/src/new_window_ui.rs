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
