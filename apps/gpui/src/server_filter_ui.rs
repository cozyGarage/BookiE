use bookie_core::TableInfo;

use super::*;

const PLACEHOLDER: &str = "e.g. created_at > now() - interval '1 day'";

#[derive(Default)]
pub(crate) struct ServerFilterState {
    pub input: Option<Entity<TextInput>>,
    pub applied: Vec<(TableInfo, String)>,
    pub shown_for: Option<TableInfo>,
}

impl GridView {
    pub(crate) fn table_where(&self, table: &TableInfo) -> String {
        self.search
            .server
            .applied
            .iter()
            .find(|(identity, _)| identity == table)
            .map(|(_, where_sql)| where_sql.clone())
            .unwrap_or_default()
    }

    fn shows_server_filter(&self) -> bool {
        self.live.is_some() && !self.grid_is_query && matches!(self.tabs.current(), Tab::Table(_))
    }

    pub(crate) fn server_filter_actions(&self) -> Vec<Action> {
        let enabled = self.shows_server_filter();
        vec![
            Action {
                id: "where",
                label: "Filter this table on the server (WHERE)".to_string(),
                enabled,
            },
            Action {
                id: "clear-where",
                label: "Clear the server WHERE filter".to_string(),
                enabled: enabled && !self.table_where(&self.catalog.grid_table).is_empty(),
            },
        ]
    }

    pub(crate) fn prepare_server_filter(&mut self, cx: &mut Context<Self>) {
        let input = match self.search.server.input.clone() {
            Some(input) => input,
            None => {
                let input = cx.new(|cx| TextInput::new(PLACEHOLDER, cx));
                cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Submit) {
                        this.apply_where(cx);
                    }
                })
                .detach();
                self.search.server.input = Some(input.clone());
                input
            }
        };
        if self.search.server.shown_for.as_ref() != Some(&self.catalog.grid_table) {
            let text = self.table_where(&self.catalog.grid_table);
            input.update(cx, |input, cx| input.set_content_quiet(&text, cx));
            self.search.server.shown_for = Some(self.catalog.grid_table.clone());
        }
    }

    pub(crate) fn focus_where(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.search.server.input.clone().filter(|_| self.shows_server_filter()) {
            Some(input) => window.focus(&input.read(cx).focus_handle(cx), cx),
            None => self.status = "Open a table to filter it on the server".into(),
        }
    }

    pub(crate) fn apply_where(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.search.server.input.clone() else {
            return;
        };
        let where_sql = input.read(cx).content().trim().to_string();
        self.load_where(where_sql, cx);
    }

    pub(crate) fn clear_where(&mut self, cx: &mut Context<Self>) {
        if let Some(input) = &self.search.server.input {
            input.update(cx, |input, cx| input.set_content_quiet("", cx));
        }
        self.load_where(String::new(), cx);
    }

    fn load_where(&mut self, where_sql: String, cx: &mut Context<Self>) {
        let Some(session) = self.live.clone().filter(|_| self.shows_server_filter()) else {
            self.status = "Open a table to filter it on the server".into();
            return;
        };
        if self.reject_view_edits() {
            cx.notify();
            return;
        }
        let table = self.catalog.grid_table.clone();
        let sort = self.table_sort(&table).cloned();
        let current_session = session.clone();
        self.status = format!("Loading {} from the server…", table_label(&table));
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                crate::catalog_nav::load(&session, &table, sort.as_ref(), &where_sql, &cancel)
                    .map(|loaded| (loaded, where_sql))
            },
            move |this, result| {
                let ((identity, source), where_sql) = match result {
                    Ok(loaded) => loaded,
                    Err(message) => {
                        this.status = format!("Could not apply the WHERE filter: {message}");
                        return;
                    }
                };
                if !this.is_current_session(&current_session)
                    || this.catalog.grid_table != identity
                    || this.grid_is_query
                    || this.reject_view_edits()
                {
                    return;
                }
                this.search.server.applied.retain(|(table, _)| *table != identity);
                if !where_sql.is_empty() {
                    this.search.server.applied.push((identity.clone(), where_sql.clone()));
                }
                this.show_loaded_table(identity, source);
                this.status = if where_sql.is_empty() {
                    "Showing the table without a WHERE filter".into()
                } else {
                    format!("Loaded {} rows WHERE {where_sql}", thousands(this.dataset_count()))
                };
            },
        );
    }

    fn sort_chip(&self) -> String {
        match self.table_sort(&self.catalog.grid_table).or(self.local_sort.as_ref()) {
            Some(sort) => format!("Sort: {} {}", sort.column, if sort.ascending { "ASC" } else { "DESC" }),
            None => "Sort: none".into(),
        }
    }

    pub(crate) fn filter_rows(&self, local: Option<Div>, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let server = self
            .search
            .server
            .input
            .clone()
            .filter(|_| self.shows_server_filter())
            .map(|input| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .pb_2()
                    .child(div().font_family("monospace").child("WHERE"))
                    .child(div().flex_1().min_w_0().font_family("monospace").child(input))
                    .child(
                        button("apply-where", "Apply", theme).on_click(cx.listener(|this, _, _, cx| {
                            this.apply_where(cx);
                            cx.notify();
                        })),
                    )
                    .child(
                        button("clear-where", "Clear", theme).on_click(cx.listener(|this, _, _, cx| {
                            this.clear_where(cx);
                            cx.notify();
                        })),
                    )
                    .child(
                        div()
                            .px_2()
                            .rounded(px(size::CONTROL_RADIUS))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .text_color(rgb(theme.muted_foreground))
                            .child(self.sort_chip()),
                    )
            });
        (server.is_some() || local.is_some()).then(|| div().flex().flex_col().children(server).children(local))
    }
}
