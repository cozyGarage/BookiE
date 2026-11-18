use std::time::SystemTime;

use bookie_app_services::statement_history::{self, HistoryQuery, HistoryRow, StatusFilter, TimeWindow};
use bookie_gpui::theme::{Theme, size};
use gpui::{Context, Div, Entity, Stateful, Window, div, prelude::*, px, rgb};

use crate::GridView;
use crate::assets::icon;
use crate::chrome::{ghost_icon, one_line};
use crate::connection_form::{choice_chip, choice_row};
use crate::text_input::{InputEvent, TextInput};

static PRUNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const PAGE_LIMIT: usize = 200;

pub(crate) struct HistoryPage {
    pub search: Entity<TextInput>,
    pub status: StatusFilter,
    pub window: TimeWindow,
    pub rows: Vec<HistoryRow>,
    generation: u64,
}

impl HistoryPage {
    pub fn new(cx: &mut Context<GridView>) -> Self {
        let search = cx.new(|cx| TextInput::new("Search statements", cx));
        cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Changed) {
                this.reload_history_page(cx);
            }
        })
        .detach();
        Self {
            search,
            status: StatusFilter::All,
            window: TimeWindow::Any,
            rows: Vec::new(),
            generation: 0,
        }
    }
}

pub(crate) fn block_on_history<T: Send + 'static>(
    work: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
) -> Result<T, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(work)
}

pub(crate) fn ago(seconds: u64) -> String {
    match seconds {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86399 => format!("{} h ago", seconds / 3600),
        _ => format!("{} d ago", seconds / 86400),
    }
}

fn meta(row: &HistoryRow) -> String {
    let elapsed = SystemTime::now()
        .duration_since(row.executed_at)
        .map_or(0, |elapsed| elapsed.as_secs());
    let took = row.duration_ms.map_or(String::new(), |ms| format!(" · {ms} ms"));
    format!("{}{} · {}", row.connection_name, took, ago(elapsed))
}

fn pinned_first(mut rows: Vec<HistoryRow>) -> Vec<HistoryRow> {
    rows.sort_by_key(|row| !row.pinned);
    rows
}

impl GridView {
    pub(crate) fn reload_history(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let recent = cx
                .background_executor()
                .spawn(async move {
                    block_on_history(async {
                        if !PRUNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                            let _ = statement_history::prune().await;
                        }
                        statement_history::recent(8).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(recent) = recent {
                    this.history = recent;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn reload_history_page(&mut self, cx: &mut Context<Self>) {
        self.history_page.generation += 1;
        let generation = self.history_page.generation;
        let query = HistoryQuery {
            text: self.history_page.search.read(cx).content().to_string(),
            status: self.history_page.status,
            window: self.history_page.window,
            limit: PAGE_LIMIT,
            ..HistoryQuery::default()
        };
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_executor()
                .spawn(async move { block_on_history(statement_history::search(query)) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.history_page.generation != generation {
                    return;
                }
                match rows {
                    Ok(rows) => this.history_page.rows = pinned_first(rows),
                    Err(message) => this.status = message,
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn change_history(
        &mut self,
        work: impl std::future::Future<Output = Result<(), String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { block_on_history(work) }).await;
            let _ = this.update(cx, |this, cx| {
                if let Err(message) = done {
                    this.status = message;
                }
                this.reload_history_page(cx);
                this.reload_history(cx);
            });
        })
        .detach();
    }

    fn reuse_statement(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.open_query();
        self.sync_grid_owner();
        self.sql_input.update(cx, |input, cx| input.set_value(query, window, cx));
        cx.notify();
    }

    fn history_status_chip(&self, id: &'static str, label: &'static str, status: StatusFilter, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        choice_chip(id, label, self.history_page.status == status, theme).on_click(cx.listener(
            move |this, _, _, cx| {
                this.history_page.status = status;
                this.reload_history_page(cx);
            },
        ))
    }

    fn history_window_chip(&self, id: &'static str, label: &'static str, window: TimeWindow, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        choice_chip(id, label, self.history_page.window == window, theme).on_click(cx.listener(
            move |this, _, _, cx| {
                this.history_page.window = window;
                this.reload_history_page(cx);
            },
        ))
    }

    fn history_row(&self, index: usize, row: &HistoryRow, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let (id, pinned, query) = (row.id, row.pinned, row.query.clone());
        let dot = if row.succeeded { theme.success } else { theme.destructive };
        let pin = ghost_icon(("history-pin", index), "pin", if pinned { "Unpin" } else { "Pin" }, theme)
            .when(pinned, |button| button.bg(rgb(theme.active_row)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.change_history(statement_history::set_pinned(id, !pinned), cx);
            }));
        let delete = ghost_icon(("history-delete", index), "trash-2", "Delete", theme)
            .on_click(cx.listener(
            move |this, _, _, cx| this.change_history(statement_history::delete(id), cx),
        ));
        div()
            .id(("history-row", index))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(size::LIST_ROW))
            .rounded(px(theme.radius))
            .hover(move |style| style.bg(rgb(theme.accent)))
            .child(icon(if row.succeeded { "circle" } else { "x" }, dot))
            .child(
                div()
                    .id(("history-statement", index))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| this.reuse_statement(&query, window, cx)))
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .overflow_hidden()
                            .text_size(px(size::TEXT_SMALL))
                            .font_family("monospace")
                            .child(one_line(&row.query)),
                    )
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .overflow_hidden()
                            .text_size(px(size::TEXT_SMALL))
                            .text_color(rgb(theme.muted_foreground))
                            .child(meta(row)),
                    ),
            )
            .child(pin)
            .child(delete)
    }

    pub(crate) fn history_page(&mut self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let rows: Vec<Stateful<Div>> = self
            .history_page
            .rows
            .clone()
            .iter()
            .enumerate()
            .map(|(index, row)| self.history_row(index, row, theme, cx))
            .collect();
        let filters = choice_row(
            theme,
            "Show",
            [
                self.history_status_chip("history-all", "All", StatusFilter::All, theme, cx),
                self.history_status_chip("history-ok", "Succeeded", StatusFilter::Succeeded, theme, cx),
                self.history_status_chip("history-failed", "Failed", StatusFilter::Failed, theme, cx),
                self.history_status_chip("history-cancelled", "Cancelled", StatusFilter::Cancelled, theme, cx),
            ],
        );
        let windows = choice_row(
            theme,
            "When",
            [
                self.history_window_chip("history-any", "Any time", TimeWindow::Any, theme, cx),
                self.history_window_chip("history-day", "24 hours", TimeWindow::Day, theme, cx),
                self.history_window_chip("history-week", "7 days", TimeWindow::Week, theme, cx),
                self.history_window_chip("history-month", "30 days", TimeWindow::Month, theme, cx),
            ],
        );
        let body = if rows.is_empty() {
            div()
                .p_4()
                .text_color(rgb(theme.muted_foreground))
                .child("No statements match")
        } else {
            div().flex().flex_col().gap_1().children(rows)
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .p_6()
            .bg(rgb(theme.background))
            .child(div().h(px(size::FORM_CONTROL)).child(self.history_page.search.clone()))
            .child(filters)
            .child(windows)
            .child(div().id("history-list").flex_1().min_h_0().overflow_y_scroll().child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pinned: bool) -> HistoryRow {
        HistoryRow {
            id: 1,
            query: "SELECT 1".into(),
            connection_name: "demo".into(),
            executed_at: SystemTime::now(),
            duration_ms: Some(3),
            succeeded: true,
            pinned,
        }
    }

    #[test]
    fn elapsed_time_reads_in_the_largest_whole_unit() {
        assert_eq!(ago(5), "just now");
        assert_eq!(ago(120), "2 min ago");
        assert_eq!(ago(7200), "2 h ago");
        assert_eq!(ago(200_000), "2 d ago");
    }

    #[test]
    fn pinned_statements_come_first_and_keep_their_order() {
        let mut older = row(false);
        older.id = 2;
        let rows = pinned_first(vec![row(false), older, {
            let mut pinned = row(true);
            pinned.id = 3;
            pinned
        }]);
        assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![3, 1, 2]);
    }

    #[test]
    fn a_row_says_where_how_long_and_when() {
        assert_eq!(meta(&row(false)), "demo · 3 ms · just now");
    }
}
