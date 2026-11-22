use bookie_gpui::messages::MessageKind;
use gpui::{Context, Div, MouseDownEvent, Stateful, Window, div, prelude::*, px, rgb};

use crate::assets::icon;
use crate::chrome::{SQL, TOOLBAR_HEIGHT, ghost, ghost_icon, primary_button};
use crate::{GridView, thousands};
use bookie_gpui::shell::{ResultView, Run, Tab};
use bookie_gpui::theme::{Theme, size};

impl GridView {
    pub(crate) fn query_editor_toolbar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(size::FORM_CONTROL))
            .px_2()
            .flex_none()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(
                primary_button("run-query", "Run", "Run query (Ctrl+Enter)", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.run_query(cx);
                        cx.notify();
                    },
                )),
            )
            .child(
                ghost_icon("explain-query", "info", "Explain the query", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.explain_query(cx);
                        cx.notify();
                    },
                )),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(rgb(theme.muted_foreground))
                    .child(self.catalog.connection.clone()),
            )
    }

    pub(crate) fn query_editor_pane(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .when(self.panel.open, |pane| pane.h(gpui::relative(1.0 - self.panel.ratio)))
            .when(!self.panel.open, |pane| pane.flex_1())
            .on_scroll(cx.listener(|this, event: &gpui::ScrollEvent, _, cx| {
                if !event.modifiers.control {
                    return;
                }
                let lines = event.delta.pixel_delta(px(size::EDITOR_LINE)).y;
                let delta = if lines > px(0.0) { 1 } else { -1 };
                this.zoom_editor(Some(delta), cx);
                cx.stop_propagation();
            }))
            .child(self.query_editor_toolbar(theme, cx))
            .children(self.parameters_bar(theme, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .gap_3()
                    .overflow_hidden()
                    .children(self.live.is_none().then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .text_color(rgb(theme.muted_foreground))
                            .children(
                                (1..=SQL.len()).map(|number| div().h(px(size::EDITOR_LINE)).child(number.to_string())),
                            )
                    }))
                    .children(self.live.is_none().then(|| {
                        div().flex().flex_col().children(
                            SQL.iter()
                                .map(|line| div().h(px(size::EDITOR_LINE)).child((*line).to_string())),
                        )
                    }))
                    .children(self.live.is_some().then(|| {
                        div()
                            .flex_1()
                            .when(!self.fonts.settings.editor_family.is_empty(), |pane| {
                                pane.font_family(self.fonts.settings.editor_family.clone())
                            })
                            .text_size(px(self.fonts.settings.editor_size))
                            .child(gpui_kit::component::input::Editor::new(&self.sql_input).h(gpui::relative(1.0)))
                    })),
            )
    }

    pub(crate) fn query_split_divider(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("split-divider")
            .h(px(size::SM))
            .w_full()
            .flex_none()
            .cursor_row_resize()
            .bg(rgb(theme.border))
            .hover(move |style| style.bg(rgb(theme.info)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.panel.resizing = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    pub fn query_page(&mut self, theme: Theme, grid: Div, cx: &mut Context<Self>) -> Div {
        let editor = self.query_editor_pane(theme, cx);
        let divider = self.query_split_divider(theme, cx);
        let result = self.result_pane(theme, grid, cx);
        let tools = self.tool_bars(theme, cx);
        div()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .children(tools)
            .child(editor)
            .children(self.panel.open.then_some(divider))
            .child(result)
    }

    pub(crate) fn result_tab_item(
        &self,
        index: usize,
        run: &Run,
        active: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let tab = div()
            .id(("run", index))
            .flex()
            .items_center()
            .flex_none()
            .h(px(size::SMALL_CONTROL))
            .px_2()
            .rounded(px(theme.radius))
            .border_1()
            .cursor_pointer()
            .child(match run.measured() {
                Some(took) => format!("{} · {} rows · {took}", run.label(), thousands(run.rows)),
                None => format!("{} · {} rows", run.label(), thousands(run.rows)),
            })
            .child(
                div()
                    .id(("close-run", index))
                    .px_1()
                    .rounded(px(theme.radius))
                    .hover(move |style| style.bg(rgb(theme.muted)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            this.close_run(index);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(icon("x", theme.muted_foreground)),
            );
        let tab = if active {
            tab.border_color(rgb(theme.border))
                .bg(rgb(theme.background))
                .shadow_sm()
        } else {
            tab.border_color(rgb(theme.muted))
                .text_color(rgb(theme.muted_foreground))
                .hover(move |style| style.bg(rgb(theme.accent)))
        };
        tab.on_click(cx.listener(move |this, _, _, cx| {
            this.select_run(index);
            cx.notify();
        }))
    }

    pub(crate) fn result_view_switch(
        &self,
        id: &'static str,
        label: &'static str,
        view: ResultView,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.panel.view == view;
        let button = div()
            .id(id)
            .flex()
            .items_center()
            .h(px(size::COMPACT_CONTROL))
            .px_2()
            .rounded(px(theme.radius))
            .cursor_pointer()
            .child(label);
        let button = if selected {
            button.bg(rgb(theme.accent)).text_color(rgb(theme.foreground))
        } else {
            button.text_color(rgb(theme.muted_foreground))
        };
        button.on_click(cx.listener(move |this, _, _, cx| {
            this.panel.view = view;
            cx.notify();
        }))
    }

    pub(crate) fn result_header(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let run_tabs = self
            .run_tabs
            .slots
            .iter()
            .enumerate()
            .map(|(index, slot)| self.result_tab_item(index, &slot.run, index == self.run_tabs.active, theme, cx));
        div()
            .flex()
            .items_center()
            .gap_1()
            .h(px(TOOLBAR_HEIGHT))
            .px_2()
            .flex_none()
            .bg(rgb(theme.chrome))
            .border_y_1()
            .border_color(rgb(theme.border))
            .child(icon("pin", theme.muted_foreground))
            .child(div().w(px(size::HAIRLINE)).h(px(size::LG)).bg(rgb(theme.border)))
            .child(
                div()
                    .id("result-runs")
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .overflow_x_scroll()
                    .track_scroll(&self.run_tabs.scroll)
                    .children(run_tabs),
            )
            .child(self.result_view_switch("view-table", "Table Data", ResultView::Table, theme, cx))
            .child(self.result_view_switch("view-summary", "Summary", ResultView::Summary, theme, cx))
            .child(self.result_view_switch("view-messages", "Messages", ResultView::Messages, theme, cx))
            .children(
                (!self.panel.plan.is_empty())
                    .then(|| self.result_view_switch("view-plan", "Plan", ResultView::Plan, theme, cx)),
            )
            .child(
                ghost("toggle-results", "", theme)
                    .child(icon(
                        if self.panel.open {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        theme.muted_foreground,
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.panel.open = !this.panel.open;
                        cx.notify();
                    })),
            )
    }

    pub(crate) fn result_messages_body(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div().flex_1().min_h_0().flex().flex_col().child(
            div()
                .id("messages")
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .p_4()
                .overflow_y_scroll()
                .children(
                    self.panel
                        .error_location
                        .as_ref()
                        .filter(|(_, _, id)| {
                            matches!(self.tabs.current(), Tab::Query)
                                && self
                                    .panel
                                    .messages
                                    .entries()
                                    .last()
                                    .is_some_and(|message| message.id == *id)
                        })
                        .and_then(|(sql, position, _)| {
                            let current = self.sql_input.read(cx).value();
                            crate::run_cursor_ui::error_location(current.as_ref(), sql, *position).map(
                                |(line, column)| {
                                    let expected = sql.clone();
                                    let token = crate::run_cursor_ui::error_token(sql, *position);
                                    ghost(
                                        "go-to-query-error",
                                        format!("Go to error ({}:{})", line + 1, column + 1),
                                        theme,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            if matches!(this.tabs.current(), Tab::Query)
                                                && this.sql_input.read(cx).value().as_ref() == expected
                                            {
                                                this.sql_input.update(cx, |editor, cx| {
                                                    editor.set_cursor_position(
                                                        gpui_kit::component::input::Position::new(line, column),
                                                        window,
                                                        cx,
                                                    );
                                                    if let Some(range) = token.clone() {
                                                        editor.set_selected_range(range, cx);
                                                    }
                                                    editor.focus(window, cx);
                                                });
                                            }
                                        },
                                    ))
                                },
                            )
                        }),
                )
                .children(if self.panel.messages.entries().is_empty() {
                    vec![div().text_color(rgb(theme.muted_foreground)).child("No messages yet")]
                } else {
                    self.panel
                        .messages
                        .entries()
                        .iter()
                        .rev()
                        .map(|message| {
                            let line = div().child(message.text.clone());
                            match message.kind {
                                MessageKind::Error => line.text_color(rgb(theme.warning)),
                                MessageKind::Info => line.text_color(rgb(theme.muted_foreground)),
                                MessageKind::Success => line,
                            }
                        })
                        .collect()
                }),
        )
    }

    pub(crate) fn result_summary_body(&self, run: &Run) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .p_4()
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Execution summary"))
            .children(self.live.is_none().then(|| format!("Statement: {}", SQL.join(" "))))
            .child(format!("Rows returned: {}", thousands(run.rows)))
            .child(match run.measured() {
                Some(took) => format!("Elapsed: {took}"),
                None => format!("Elapsed: {} ms (synthetic)", run.millis()),
            })
            .children(
                self.live
                    .is_none()
                    .then_some("Source: synthetic generator, no database"),
            )
    }

    pub(crate) fn result_body(&self, theme: Theme, grid: Div, cx: &mut Context<Self>) -> Div {
        if self.panel.view == ResultView::Messages {
            self.result_messages_body(theme, cx)
        } else if self.panel.view == ResultView::Plan && !self.panel.plan.is_empty() {
            self.plan_body(theme)
        } else if let Some(slot) = self
            .run_tabs
            .slots
            .get(self.run_tabs.active)
            .filter(|_| self.panel.view == ResultView::Summary)
        {
            self.result_summary_body(&slot.run)
        } else if self.run_tabs.slots.is_empty() {
            div().flex_1().p_4().child("No results")
        } else {
            grid
        }
    }

    pub(crate) fn result_pane(&mut self, theme: Theme, grid: Div, cx: &mut Context<Self>) -> Div {
        let header = self.result_header(theme, cx);
        let body = self.result_body(theme, grid, cx);
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .when(self.panel.open, |pane| pane.flex_1())
            .child(header)
            .children(self.panel.open.then_some(body))
    }

    pub fn viewport_rows_for(&self, window: &Window) -> usize {
        ((f32::from(window.viewport_size().height) - 250.0) / crate::ROW_HEIGHT)
            .floor()
            .max(1.0) as usize
    }
}
