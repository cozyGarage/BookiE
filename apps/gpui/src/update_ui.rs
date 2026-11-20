use bookie_app_services::update_check::{RUNNING_VERSION, UpdateCheckResult, check_for_updates, outcome_message};

use super::*;

pub(crate) fn update_status(result: &UpdateCheckResult, running: &str) -> String {
    outcome_message(result, running)
}

impl GridView {
    pub(crate) fn update_actions(&self) -> Vec<Action> {
        vec![Action {
            id: "check-for-updates",
            label: "Check for updates".to_string(),
            enabled: true,
        }]
    }

    pub(crate) fn check_for_updates_now(&mut self, cx: &mut Context<Self>) {
        self.status = "Checking for updates…".into();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { check_for_updates(RUNNING_VERSION) })
                .await;
            this.update(cx, |this, cx| {
                this.status = update_status(&result, RUNNING_VERSION);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "update_ui_tests.rs"]
mod tests;
