use bookie_app_services::closed_drafts::ClosedDraft;
use bookie_gpui::shell::Tab;
use gpui::{App, Context, Window};

use crate::GridView;

impl GridView {
    pub(crate) fn remember_query_draft(&mut self, cx: &App) {
        let Some(drafts) = &self.closed_drafts else {
            return;
        };
        let draft = ClosedDraft {
            sql: self.sql_input.read(cx).value().to_string(),
            connection: self
                .live
                .as_ref()
                .and_then(|live| live.saved.as_ref().map(|(id, _)| *id)),
        };
        if let Err(error) = drafts.remember(draft) {
            tracing::warn!(%error, "could not keep the closed query draft");
        }
    }

    pub(crate) fn remember_open_query_draft(&mut self, cx: &App) {
        if !self.sql_input.read(cx).value().trim().is_empty() {
            self.remember_query_draft(cx);
        }
    }

    pub(crate) fn forget_reopened_draft(&mut self, cx: &App) {
        if let Some(drafts) = &self.closed_drafts {
            drafts.forget_latest_if(self.sql_input.read(cx).value().as_ref());
        }
    }

    pub(crate) fn restore_query_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(draft) = self.closed_drafts.as_ref().and_then(|drafts| drafts.take_latest()) else {
            return false;
        };
        self.tabs.open_query();
        self.sync_grid_owner();
        self.sql_input
            .update(cx, |input, cx| input.set_value(draft.sql, window, cx));
        self.status = "Reopened the last closed query".into();
        true
    }

    pub(crate) fn go_to_last_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(position) = self.last_edit else {
            self.status = "There is no edit to return to".into();
            return;
        };
        if self.tabs.current() != &Tab::Query {
            self.tabs.open_query();
            self.sync_grid_owner();
        }
        self.sql_input
            .update(cx, |input, cx| input.set_cursor_position(position, window, cx));
    }
}
