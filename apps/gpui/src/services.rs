use super::*;

impl GridView {
    pub(crate) fn sample_pubsub(&mut self, pattern: String, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else {
            return;
        };
        self.status = format!(
            "Listening to {pattern} for {} s…",
            bookie_gpui::pubsub_view::SAMPLE_SECONDS
        );
        let target = pattern.clone();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                live.sample_pubsub(
                    std::slice::from_ref(&target),
                    bookie_gpui::pubsub_view::SAMPLE_MESSAGES,
                    Duration::from_secs(bookie_gpui::pubsub_view::SAMPLE_SECONDS),
                    &cancel,
                )
            },
            move |this, outcome| {
                let lines = bookie_gpui::pubsub_view::describe(&pattern, outcome);
                if let Some((_, headline)) = lines.first() {
                    this.status = headline.clone();
                }
                for (kind, text) in lines {
                    this.panel.messages.push(kind, text);
                }
            },
        );
    }

    pub(crate) fn kafka_topics(&mut self, broker: String, cx: &mut Context<Self>) {
        self.status = format!("Listing topics at {broker}…");
        self.begin_background(
            Work::Load,
            cx,
            move |_| bookie_gpui::net_check::kafka_topics(&broker),
            |this, outcome| {
                let (kind, text) = match outcome {
                    Ok(text) => (MessageKind::Success, text),
                    Err(error) => (MessageKind::Error, format!("Kafka · {error}")),
                };
                this.panel.messages.push(kind, text.clone());
                this.status = text;
            },
        );
    }

    pub(crate) fn check_tcp(&mut self, address: String, cx: &mut Context<Self>) {
        self.status = format!("Checking {address}…");
        let target = address.clone();
        self.begin_background(
            Work::Load,
            cx,
            move |_| bookie_gpui::net_check::run_check(&target),
            move |this, outcome| {
                let (up, text) = bookie_gpui::net_check::record(&mut this.check_histories, &address, outcome);
                let kind = if up { MessageKind::Success } else { MessageKind::Error };
                this.panel.messages.push(kind, text.clone());
                this.status = text;
            },
        );
    }

    pub(crate) fn show_status(&mut self) {
        let Some(live) = self.live.clone() else {
            self.status = "No active database connection.".into();
            return;
        };
        let overview = match live.overview() {
            Ok(overview) => overview,
            Err(error) => {
                let message = format!("Could not read connection status: {error}");
                self.status = message.clone();
                self.panel.messages.push(MessageKind::Error, message);
                return;
            }
        };
        let lines = bookie_gpui::status_view::describe(&overview);
        if let Some((_, headline)) = lines.first() {
            self.status = headline.clone();
        }
        for (kind, text) in lines {
            self.panel.messages.push(kind, text);
        }
    }

    pub(crate) fn show_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.live.as_ref().is_some_and(|live| live.driver_id == "postgres") {
            self.tabs.open_query();
            self.sync_grid_owner();
            self.sql_input.update(cx, |input, cx| {
                input.set_value(bookie_app_services::pg_sessions::SESSIONS_SQL, window, cx)
            });
            self.run_query(cx);
            return;
        }
        self.show_activity(bookie_core::ActivityQuery::Sessions, window, cx);
    }

    pub(crate) fn show_activity(
        &mut self,
        kind: bookie_core::ActivityQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = &self.live else { return };
        if let Ok(sql) = bookie_core::activity_sql(&live.driver_id, kind, None) {
            self.tabs.open_query();
            self.sync_grid_owner();
            self.sql_input.update(cx, |input, cx| input.set_value(&sql, window, cx));
            self.run_query(cx);
        }
    }

    pub(crate) fn cancel_server_process(&mut self, pid: i64, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_ref() else {
            self.status = "No active database connection.".into();
            return;
        };
        let sql = match live.driver_id.as_str() {
            "postgres" => format!("SELECT pg_cancel_backend({pid});"),
            "mysql" => format!("KILL QUERY {pid};"),
            _ => {
                self.status = "Cancelling processes is not supported for this database engine.".into();
                return;
            }
        };
        self.status = format!("Cancelling server process {pid}…");
        self.run_sql(sql, cx);
    }

    pub(crate) fn cancel_selected_process(&mut self, cx: &mut Context<Self>) {
        let Some((row, _)) = self.active_row_with_names() else {
            self.status = "Select a process row to cancel".into();
            return;
        };
        let pid = row
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("pid"))
            .and_then(|(_, v)| match v {
                bookie_core::Value::Int(n) => Some(*n),
                _ => None,
            });
        let Some(pid) = pid else {
            self.status = "No process PID found in selected row".into();
            return;
        };
        self.cancel_server_process(pid, cx);
    }
}
