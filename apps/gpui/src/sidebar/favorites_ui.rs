use std::path::PathBuf;

use bookie_app_services::favorites::{self, Favorite};
use bookie_app_services::service_runtime::block_on;
use uuid::Uuid;

use crate::*;

#[derive(Default)]
pub(crate) struct FavoritesState {
    pub list: Vec<Favorite>,
    pub path: Option<PathBuf>,
    requests: Ticket,
}

impl GridView {
    pub(crate) fn refresh_favorites(&mut self, cx: &mut Context<Self>) {
        let path = self.favorites.path.clone();
        let ticket = self.favorites.requests.next();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { block_on(async move { favorites::list(path.as_deref()).await }) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(list) if this.favorites.requests.is_current(ticket) => this.favorites.list = list,
                    Ok(_) => {}
                    Err(message) if this.favorites.is_latest(ticket) => this.report_failure("read favorites", &message),
                    Err(_) => {}
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn save_favorite(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.tabs.current(), Tab::Query) {
            self.status = "Open a query tab to save a favorite".into();
            return;
        }
        let sql = self.sql_input.read(cx).value().to_string();
        if sql.trim().is_empty() {
            self.status = "Type a statement first".into();
            return;
        }
        let (path, driver) = (
            self.favorites.path.clone(),
            self.live.as_ref().map(|s| s.driver_id.clone()),
        );
        let name = favorites::default_name(&sql);
        let ticket = self.favorites.requests.next();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { block_on(async move { favorites::save(path.as_deref(), &sql, driver).await }) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(list) => {
                        if this.favorites.requests.is_current(ticket) {
                            this.favorites.list = list;
                        }
                        this.status = format!("Saved favorite \u{201c}{name}\u{201d}");
                        this.notices
                            .push(this.status.clone(), crate::notifications::NoticeKind::Success);
                    }
                    Err(message) => this.report_failure("save the favorite", &message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn load_favorite(&mut self, sql: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.open_query();
        self.sync_grid_owner();
        self.set_sql_text(sql.to_owned(), window, cx);
        self.status = "Loaded the favorite into the editor".into();
    }

    pub(crate) fn delete_favorite(&mut self, id: Uuid, name: String, cx: &mut Context<Self>) {
        let path = self.favorites.path.clone();
        let ticket = self.favorites.requests.next();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { block_on(async move { favorites::delete(path.as_deref(), id).await }) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(list) => {
                        if this.favorites.requests.is_current(ticket) {
                            this.favorites.list = list;
                        }
                        this.status = format!("Deleted favorite \u{201c}{name}\u{201d}");
                        this.notices
                            .push(this.status.clone(), crate::notifications::NoticeKind::Success);
                    }
                    Err(message) => this.report_failure("delete the favorite", &message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::FavoritesState;

    #[test]
    fn only_the_latest_favorites_request_may_replace_the_list() {
        let mut state = FavoritesState::default();
        let first = state.requests.next();
        let second = state.requests.next();
        assert!(!state.requests.is_current(first));
        assert!(state.requests.is_current(second));
    }
}
