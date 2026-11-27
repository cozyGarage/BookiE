use bookie_core::TableInfo;

use crate::surfaces::{ButtonKind, compact_button, truncated};
use crate::*;

const PLACEHOLDER: &str = "e.g. created_at > now() - interval '1 day'";

#[derive(Default)]
pub(crate) struct ServerFilterState {
    pub input: Option<Entity<TextInput>>,
    pub applied: Vec<(TableInfo, String)>,
    pub shown_for: Option<TableInfo>,
    visible: bool,
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
        self.shows_live_table() && matches!(self.tabs.current(), Tab::Table(_))
    }

    pub(crate) fn server_filter_actions(&self) -> Vec<Action> {
        let enabled = self.shows_server_filter();
        vec![
            Action {
                id: "where",
                label: "Filter this table on the server with WHERE".to_string(),
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
            Some(input) => {
                self.search.server.visible = true;
                window.focus(&input.read(cx).focus_handle(cx), cx);
                cx.notify();
            }
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
        let local_sort = self.local_sort.clone();
        let keep_local_sort = local_sort.clone();
        let current_session = session.clone();
        self.status = format!("Loading {} from the server…", table_label(&table));
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                let (identity, source) =
                    crate::catalog_nav::load(&session, &table, sort.as_ref(), &where_sql, &cancel)?;
                let order = local_sort
                    .as_ref()
                    .and_then(|sort| crate::local_sort::resort_loaded(source.as_ref(), sort, &cancel));
                Ok::<_, String>(((identity, source), where_sql, order))
            },
            move |this, result| {
                let ((identity, source), where_sql, order) = match result {
                    Ok(loaded) => loaded,
                    Err(message) => {
                        this.report_failure("apply the WHERE filter", &message);
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
                let sort_unchanged = this.local_sort == keep_local_sort;
                this.show_loaded_table(identity, source);
                if let (true, Some(order), Some(sort)) = (sort_unchanged, order, keep_local_sort) {
                    this.local_order = Some(std::sync::Arc::new(order));
                    this.local_sort = Some(sort);
                    this.pager = this.pager.with_total(this.dataset_count());
                }
                this.status = if where_sql.is_empty() {
                    "Showing the table without a WHERE filter".into()
                } else {
                    format!("Loaded {} rows WHERE {where_sql}", thousands(this.dataset_count()))
                };
            },
        );
    }

    fn sort_chip(&self) -> Option<String> {
        self.table_sort(&self.catalog.grid_table)
            .or(self.local_sort.as_ref())
            .map(|sort| sort_chip_text(&sort.column, sort.ascending))
    }

    pub(crate) fn where_toggle(&self, theme: Theme, cx: &mut Context<Self>) -> Option<crate::surfaces::Button> {
        if !self.shows_server_filter() {
            return None;
        }
        let applied = !self.table_where(&self.catalog.grid_table).is_empty();
        Some(
            compact_button(
                ButtonKind::Ghost,
                "toggle-where",
                if applied { "WHERE active" } else { "WHERE" },
                theme,
            )
            .tooltip("Filter this table on the server")
            .on_click(cx.listener(|this, _, window, cx| {
                if this.search.server.visible {
                    this.search.server.visible = false;
                    window.focus(&this.focus, cx);
                    cx.notify();
                } else {
                    this.focus_where(window, cx);
                }
            })),
        )
    }

    pub(crate) fn server_filter_row(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        if !self.shows_server_filter() || !self.search.server.visible {
            return None;
        }
        let input = self.search.server.input.clone()?;
        Some(
            div()
                .debug_selector(|| "server-filter-row".into())
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .child(div().flex_none().font_family("monospace").child("WHERE"))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .max_w(px(size::FILTER_WIDTH))
                        .font_family("monospace")
                        .child(input),
                )
                .child(
                    compact_button(ButtonKind::Primary, "apply-where", "Apply", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.apply_where(cx))),
                )
                .child(
                    compact_button(ButtonKind::Ghost, "clear-where", "Clear", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.clear_where(cx))),
                )
                .children(self.sort_chip().map(|text| {
                    truncated("sort-chip", text)
                        .max_w(px(size::FIND_WIDTH))
                        .text_color(rgb(theme.muted_foreground))
                })),
        )
    }
}

fn sort_chip_text(column: &str, ascending: bool) -> String {
    format!(
        "Sorted by {column}, {}",
        if ascending { "ascending" } else { "descending" }
    )
}

#[cfg(test)]
mod sort_chip_tests {
    use super::sort_chip_text;

    #[test]
    fn the_chip_names_the_column_and_the_direction_in_words() {
        assert_eq!(sort_chip_text("name", false), "Sorted by name, descending");
        assert_eq!(sort_chip_text("id", true), "Sorted by id, ascending");
    }
}
