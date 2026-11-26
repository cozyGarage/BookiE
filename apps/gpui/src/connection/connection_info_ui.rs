use bookie_app_services::connection_details::short_server_version;
use gpui::Context;

use crate::GridView;

impl GridView {
    pub(crate) fn request_connection_info(&mut self, cx: &mut Context<Self>) {
        self.connection_info = None;
        self.table_sizes = Default::default();
        let Some(session) = self.live.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let query_session = session.clone();
            let info = cx
                .background_executor()
                .spawn(async move { (query_session.server_info(), query_session.table_sizes()) })
                .await;
            this.update(cx, |this, cx| {
                if this.is_current_session(&session) {
                    let (info, sizes) = info;
                    this.connection_info = Some(info);
                    this.table_sizes = sizes;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn on_table_tab(&self) -> bool {
        matches!(self.tabs.current(), bookie_gpui::shell::Tab::Table(_))
    }

    fn status_origin(&self) -> String {
        if self.on_table_tab() || self.live.is_none() {
            return self.source.origin();
        }
        self.catalog.connection.clone()
    }

    pub(crate) fn connection_chip_text(&self) -> String {
        let origin = self.status_origin();
        let Some(version) = self.connection_version() else {
            return origin;
        };
        let driver = self.live.as_ref().map(|live| format!("{} · ", live.driver_id));
        match driver.as_deref().and_then(|driver| origin.strip_prefix(driver)) {
            Some(rest) => format!("{version} · {rest}"),
            None => format!("{version} · {origin}"),
        }
    }

    pub(crate) fn connection_tooltip(&self) -> String {
        let origin = self.status_origin();
        let Some(info) = self.connection_info.as_ref() else {
            return origin;
        };
        let details = match &info.details {
            Some(details) => details.tooltip(info.version.as_deref(), info.round_trip),
            None => [
                info.version.as_ref().map(|version| format!("Server: {version}")),
                info.round_trip
                    .map(|round_trip| format!("Round trip: {} ms", round_trip.as_millis())),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n"),
        };
        if details.is_empty() {
            origin
        } else {
            format!("{origin}\n{details}")
        }
    }

    fn connection_version(&self) -> Option<String> {
        self.connection_info
            .as_ref()
            .and_then(|info| info.version.as_deref())
            .map(short_server_version)
            .filter(|version| !version.is_empty())
    }
}
