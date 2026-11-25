use bookie_gpui::keymap::Command;
use bookie_gpui::messages::MessageKind;
use gpui::{Context, Div, MouseDownEvent, Stateful, Window, div, prelude::*, px, rgb};

use crate::chrome::{SQL, TOOLBAR_HEIGHT, tip};
use crate::surfaces::{ButtonKind, button, compact_icon_button, segment, segmented};
use crate::{GridView, thousands};
use bookie_gpui::shell::{ResultView, Run, Tab};
use bookie_gpui::theme::{Theme, size};

impl GridView {
    pub(crate) fn toggle_results(&mut self) {
        if self.panel.explaining {
            self.invalidate_background();
            self.panel.explaining = false;
            self.status = "Canceled the explain".into();
        }
        self.panel.open = !self.panel.open;
    }

    pub(crate) fn format_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let driver = self
            .live
            .as_ref()
            .map_or_else(|| "postgres".to_string(), |session| session.driver_id.clone());
        let Some(grammar) = bookie_app_services::result_editability::grammar_for_driver(&driver) else {
            self.status = "Formatting is unavailable for this connection".into();
            return;
        };
        let text = self.sql_input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let settings = bookie_core::sql_syntax::script::LexicalSettings::default_for(grammar);
        let formatted = bookie_core::sql_format::format_script(&text, grammar, settings);
        if formatted != text {
            self.sql_input
                .update(cx, |input, cx| input.set_value(formatted, window, cx));
            self.status = "Formatted the SQL".into();
        }
    }

    fn query_file_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        use gpui_component::button::{Button, ButtonVariants};
        use gpui_component::menu::DropdownMenu;
        use gpui_kit::component::Sizable as _;
        let groups = crate::menus::command_groups(self, crate::menus::FILE_GROUPS);
        div().debug_selector(|| "query-file-menu".into()).child(
            Button::new("query-file-menu")
                .ghost()
                .small()
                .tooltip("Open or save SQL files, export results or import a file")
                .accessibility_label("Import / Export")
                .child(
                    div()
                        .whitespace_nowrap()
                        .text_color(rgb(theme.foreground))
                        .child("Import / Export"),
                )
                .child(crate::assets::icon("chevron-down", theme.muted_foreground))
                .dropdown_menu(crate::menus::command_menu(cx.entity().downgrade(), groups)),
        )
    }

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
                button(ButtonKind::Primary, "run-statement", "Run", theme)
                    .tooltip(tip(self.command_tip(Command::RunAtCursor)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.run_at_cursor(cx);
                        cx.notify();
                    })),
            )
            .child(
                button(ButtonKind::Ghost, "run-query", "Run all", theme)
                    .tooltip(tip(self.command_tip(Command::RunQuery)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.run_query(cx);
                        cx.notify();
                    })),
            )
            .child(
                button(ButtonKind::Ghost, "explain-query", "Explain", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.explain_query(cx);
                    cx.notify();
                })),
            )
            .child(
                button(ButtonKind::Ghost, "format-query", "Format", theme).on_click(cx.listener(
                    |this, _, window, cx| {
                        this.format_query(window, cx);
                        cx.notify();
                    },
                )),
            )
            .child(
                button(ButtonKind::Ghost, "save-query", "Save as favorite", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.save_favorite(cx);
                        cx.notify();
                    },
                )),
            )
            .child(self.query_file_menu(theme, cx))
            .child(div().flex_1())
            .child(
                div()
                    .id("query-connection-chip")
                    .debug_selector(|| "query-connection-chip".into())
                    .flex()
                    .items_center()
                    .gap_1()
                    .min_w_0()
                    .overflow_hidden()
                    .h(px(size::SMALL_CONTROL))
                    .px_2()
                    .rounded(px(theme.radius))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    .text_color(rgb(theme.muted_foreground))
                    .hover(move |style| style.bg(rgb(theme.accent)))
                    .tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new("Show connection and tunnel status").build(window, cx)
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_status();
                        cx.notify();
                    }))
                    .child(crate::assets::icon("database", theme.muted_foreground))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(self.catalog.connection.clone()),
                    ),
            )
            .children(self.schema_context_chip(theme, cx))
            .child(div().flex_1())
            .child(self.transaction_controls(theme, cx))
    }

    fn schema_context_chip(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let session = self.live.as_ref()?;
        if !bookie_app_services::schema_context::supports_schema_context(&session.driver_id) {
            return None;
        }
        let label = self.schema_context.clone().unwrap_or_else(|| "schema".to_owned());
        Some(
            div()
                .id("query-schema-chip")
                .debug_selector(|| "query-schema-chip".into())
                .flex()
                .items_center()
                .min_w_0()
                .overflow_hidden()
                .h(px(size::SMALL_CONTROL))
                .px_2()
                .rounded(px(theme.radius))
                .border_1()
                .border_color(rgb(theme.border))
                .cursor_pointer()
                .text_color(rgb(theme.muted_foreground))
                .hover(move |style| style.bg(rgb(theme.accent)))
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new("Choose the schema or database queries run in")
                        .build(window, cx)
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_picker(crate::palette_state::PaletteMode::SchemaContext, window, cx);
                    cx.notify();
                }))
                .child(div().whitespace_nowrap().text_ellipsis().child(label)),
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
                        let active = {
                            let driver = self
                                .live
                                .as_ref()
                                .map_or_else(|| "postgres".to_string(), |session| session.driver_id.clone());
                            let editor = self.sql_input.read(cx);
                            let (text, cursor, selection) =
                                (editor.value().to_string(), editor.cursor(), editor.selected_range());
                            crate::run_cursor_ui::run_source_range(&text, &driver, cursor, selection)
                        };
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .w(px(size::GUTTER_MARKER_WIDTH))
                                    .h_full()
                                    .rounded_full()
                                    .when(active.is_some(), |strip| strip.bg(rgb(theme.accent)))
                                    .when(active.is_none(), |strip| strip.bg(rgb(theme.muted))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .when(!self.fonts.settings.editor_family.is_empty(), |pane| {
                                        pane.font_family(self.fonts.settings.editor_family.clone())
                                    })
                                    .text_size(px(self.fonts.settings.editor_size))
                                    .child(
                                        gpui_kit::component::input::Editor::new(&self.sql_input).h(gpui::relative(1.0)),
                                    ),
                            )
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
            .hover(move |style| style.bg(rgb(theme.focus_ring)))
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
        let label = match (self.run_tabs.slots[index].failure.is_some(), run.measured()) {
            (true, _) => format!("{} · failed", run.label()),
            (false, Some(took)) => format!("{} · {} rows · {took}", run.label(), thousands(run.rows)),
            (false, None) => format!("{} · {} rows", run.label(), thousands(run.rows)),
        };
        let tab = segment(("run", index), label, active, theme).flex_none().child(
            compact_icon_button(("close-run", index), "x", "Close result", theme)
                .size(px(size::LG))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        this.close_run(index);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
        );
        let tab = if let Some(tip) = run.tooltip() {
            tab.tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
        } else {
            tab
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
        segment(id, label, self.panel.view == view, theme).on_click(cx.listener(move |this, _, _, cx| {
            if view == ResultView::Chart {
                this.show_chart(cx);
            }
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
            .map(|(index, slot)| self.result_tab_item(index, &slot.run, index == self.run_tabs.active, theme, cx))
            .collect::<Vec<_>>();
        let empty = run_tabs.is_empty();
        let mut views = vec![
            self.result_view_switch("view-table", "Table data", ResultView::Table, theme, cx),
            self.result_view_switch("view-summary", "Summary", ResultView::Summary, theme, cx),
            self.result_view_switch("view-messages", "Messages", ResultView::Messages, theme, cx),
        ];
        if !self.panel.plan.is_empty() {
            views.push(self.result_view_switch("view-plan", "Plan", ResultView::Plan, theme, cx));
        }
        if !self.run_tabs.slots.is_empty() {
            views.push(self.result_view_switch("view-chart", "Chart", ResultView::Chart, theme, cx));
        }
        div()
            .debug_selector(|| "results-header".into())
            .flex()
            .items_center()
            .gap_1()
            .h(px(TOOLBAR_HEIGHT))
            .px_2()
            .flex_none()
            .bg(rgb(theme.chrome))
            .border_y_1()
            .border_color(rgb(theme.border))
            .child(
                segmented(
                    [div()
                        .id("result-runs")
                        .flex()
                        .min_w_0()
                        .gap(px(size::OPTICAL))
                        .overflow_x_scroll()
                        .track_scroll(&self.run_tabs.scroll)
                        .children(run_tabs)],
                    theme,
                )
                .flex_initial()
                .min_w_0()
                .when(empty, |runs| runs.p_0()),
            )
            .child(div().flex_1())
            .child(segmented(views, theme))
            .child(
                compact_icon_button(
                    "toggle-results",
                    if self.panel.open {
                        "chevron-down"
                    } else {
                        "chevron-right"
                    },
                    if self.panel.open {
                        "Hide results"
                    } else {
                        "Show results"
                    },
                    theme,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_results();
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
                        .filter(|(_, _, id, _)| {
                            matches!(self.tabs.current(), Tab::Query)
                                && self
                                    .panel
                                    .messages
                                    .entries()
                                    .last()
                                    .is_some_and(|message| message.id == *id)
                        })
                        .and_then(|(sql, position, _, source)| {
                            let current = self.sql_input.read(cx).value();
                            let ((line, column), token) = crate::run_cursor_ui::mapped_error_location(
                                current.as_ref(),
                                sql,
                                *position,
                                source.as_ref(),
                            )?;
                            let expected = source
                                .as_ref()
                                .map_or_else(|| sql.clone(), |(snapshot, _)| snapshot.clone());
                            Some({
                                button(
                                    ButtonKind::Ghost,
                                    "go-to-query-error",
                                    format!("Go to error ({}:{})", line + 1, column + 1),
                                    theme,
                                )
                                .debug_selector(|| "go-to-query-error".into())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if matches!(this.tabs.current(), Tab::Query)
                                        && this.sql_input.read(cx).value().as_ref() == expected
                                    {
                                        this.sql_input.update(cx, |editor, cx| {
                                            editor.set_cursor_position(
                                                gpui_kit::component::input::Position::new(line, column),
                                                window,
                                                cx,
                                            );
                                            editor.set_selected_range(token.clone(), cx);
                                            editor.focus(window, cx);
                                        });
                                    }
                                }))
                            })
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
                                MessageKind::Error => line.text_color(rgb(theme.destructive)),
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
            .child({
                let line = match run.measured() {
                    Some(took) => format!("Elapsed: {took}"),
                    None => format!("Elapsed: {} ms (synthetic)", run.millis()),
                };
                let el = div().id("execution-summary-elapsed").child(line);
                if let Some(tip) = run.tooltip() {
                    el.tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                } else {
                    el
                }
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
        } else if let Some(failure) = self
            .run_tabs
            .slots
            .get(self.run_tabs.active)
            .and_then(|slot| slot.failure.as_ref())
        {
            div().flex_1().p_4().child(failure.clone())
        } else if self.panel.view == ResultView::Plan && !self.panel.plan.is_empty() {
            self.plan_body(theme)
        } else if self.panel.view == ResultView::Chart && !self.run_tabs.slots.is_empty() {
            self.chart_body(theme, cx)
        } else if let Some(slot) = self
            .run_tabs
            .slots
            .get(self.run_tabs.active)
            .filter(|_| self.panel.view == ResultView::Summary)
        {
            self.result_summary_body(&slot.run)
        } else if self.run_tabs.slots.is_empty() {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1()
                .p_4()
                .text_color(rgb(theme.muted_foreground))
                .child(div().pb_2().child("Run a query to see results"))
                .children(
                    [Command::RunAtCursor, Command::RunQuery, Command::Palette]
                        .into_iter()
                        .filter_map(|command| {
                            let keys = self.keymap.binding(command)?.label();
                            Some(shortcut_hint(command.title(), keys, theme))
                        }),
                )
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

fn shortcut_hint(label: &'static str, keys: String, theme: Theme) -> Div {
    let selector = format!("shortcut-keycap-{keys}");
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .w(px(size::SHORTCUT_HINT_WIDTH))
        .text_size(px(size::TEXT_SMALL))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
        .child(
            div()
                .debug_selector(move || selector)
                .flex_none()
                .w(px(size::SHORTCUT_KEYCAP_WIDTH))
                .flex()
                .justify_center()
                .py_1()
                .rounded(px(size::CONTROL_RADIUS))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.chrome))
                .child(keys),
        )
}
