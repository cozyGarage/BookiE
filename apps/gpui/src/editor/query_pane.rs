use bookie_gpui::keymap::Command;
use bookie_gpui::messages::MessageKind;
use gpui::{Context, Div, MouseDownEvent, Stateful, Window, div, prelude::*, px, rgb};
use gpui_component::menu::DropdownMenu as _;

use crate::chrome::{SQL, TOOLBAR_HEIGHT};
use crate::surfaces::{
    Bar, ButtonKind, ListState, button, compact_icon_button, list_state, picker_chip, segment, segmented,
    select_button, shortcut, tip, truncated,
};
use crate::{GridView, thousands};
use bookie_gpui::shell::{ResultView, Run, Tab};
use bookie_gpui::theme::{Theme, size};

impl GridView {
    pub(crate) fn toggle_results(&mut self) {
        if self.panel.explaining {
            self.invalidate_query();
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
            self.set_sql_text(formatted, window, cx);
            self.status = "Formatted the SQL".into();
        }
    }

    fn query_file_menu(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = crate::menus::command_groups(self, crate::menus::FILE_GROUPS);
        crate::surfaces::menu_button(
            "query-file-menu",
            crate::surfaces::MenuTrigger::new("Open or save SQL files, export results or import a file", Bar::Small)
                .icon("folder")
                .label((!compact).then_some("File")),
            theme,
        )
        .dropdown_menu(crate::menus::command_menu(cx.entity().downgrade(), groups))
    }

    pub(crate) fn query_editor_toolbar(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(|| "query-editor-toolbar".into())
            .flex()
            .items_center()
            .when(compact, |toolbar| toolbar.gap_1())
            .when(!compact, |toolbar| toolbar.gap_2())
            .min_w_0()
            .h(px(size::FORM_CONTROL))
            .px_2()
            .flex_none()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(self.query_run_controls(theme, compact, cx))
            .child(self.query_editor_tools(compact, theme, cx))
            .child(self.query_file_menu(theme, compact, cx))
            .child(div().flex_1().min_w_0())
            .when(compact, |toolbar| toolbar.child(self.query_context_menu(theme, cx)))
            .when(!compact, |toolbar| {
                toolbar
                    .child(self.query_connection_chip(theme, cx))
                    .children(self.schema_context_chip(theme, cx))
            })
            .child(div().flex_1().min_w_0())
            .child(self.transaction_controls(theme, compact, cx))
    }

    fn query_run_controls(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let selected = !self.sql_input.read(cx).selected_range().is_empty();
        let running = self
            .panel
            .running
            .map(|(_, at)| at)
            .or_else(|| self.transaction_statement().map(|statement| statement.started))
            .map(|at| at.elapsed().as_secs());
        let stopping = match self.transaction_statement() {
            Some(statement) => statement.cancel.is_cancelled(),
            None => self.queries.cancel.is_cancelled(),
        };
        div()
            .flex()
            .flex_none()
            .gap_1()
            .items_center()
            .min_w(px(if compact {
                size::QUERY_RUN_AREA_COMPACT_WIDTH
            } else {
                size::QUERY_RUN_AREA_WIDTH
            }))
            .when(running.is_none() && compact, |bar| {
                bar.child(
                    button(ButtonKind::Primary, "run-statement", "Run", theme)
                        .tooltip(if selected {
                            bookie_gpui::keymap::with_shortcut(
                                "Run selection",
                                self.keymap.binding(Command::RunAtCursor),
                            )
                        } else {
                            self.command_tip(Command::RunAtCursor)
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_at_cursor(cx);
                            cx.notify();
                        })),
                )
                .child(self.query_run_options(selected, theme, cx))
            })
            .when(running.is_none() && !compact, |bar| {
                bar.child(
                    button(
                        ButtonKind::Primary,
                        "run-statement",
                        if selected { "Run selection" } else { "Run statement" },
                        theme,
                    )
                    .tooltip(self.command_tip(Command::RunAtCursor))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.run_at_cursor(cx);
                        cx.notify();
                    })),
                )
                .child(
                    button(ButtonKind::Ghost, "run-query", "Run all", theme)
                        .tooltip(self.command_tip(Command::RunQuery))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_query(cx);
                            cx.notify();
                        })),
                )
            })
            .children(running.map(|seconds| {
                div()
                    .text_color(rgb(theme.muted_foreground))
                    .whitespace_nowrap()
                    .child(if compact {
                        format!("{seconds} s")
                    } else {
                        format!("Running · {seconds} s")
                    })
            }))
            .when(running.is_some(), |bar| {
                if stopping {
                    bar.child(crate::surfaces::unavailable_button(
                        ButtonKind::Ghost,
                        "stop-query",
                        "Stopping…",
                        "Waiting for the database",
                        theme,
                    ))
                } else {
                    bar.child(
                        button(ButtonKind::Destructive, "stop-query", "Stop", theme).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.stop_query();
                                cx.notify();
                            },
                        )),
                    )
                }
            })
    }

    fn query_run_options(&self, selected: bool, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let at_cursor = self.action_shortcut("run-at-cursor");
        let all = self.action_shortcut("run");
        let label = |id| bookie_gpui::palette::menu_label(id).unwrap_or(id);
        let (statement, run_all) = (label("run-at-cursor"), label("run"));
        select_button("query-run-options", None, "Run options", Bar::Small, theme).dropdown_menu(move |menu, _, _| {
            let selection_owner = owner.clone();
            let statement_owner = owner.clone();
            let all_owner = owner.clone();
            menu.item(
                crate::menus::item("Run selection", None, at_cursor.clone().filter(|_| selected), false)
                    .disabled(!selected)
                    .on_click(move |_, _, cx| {
                        let _ = selection_owner.update(cx, |view, cx| view.run_at_cursor(cx));
                    }),
            )
            .item(
                crate::menus::item(statement, Some("play"), at_cursor.clone().filter(|_| !selected), false).on_click(
                    move |_, _, cx| {
                        let _ = statement_owner.update(cx, |view, cx| view.run_statement_at_cursor(cx));
                    },
                ),
            )
            .item(
                crate::menus::item(run_all, None, all.clone(), false).on_click(move |_, _, cx| {
                    let _ = all_owner.update(cx, |view, cx| view.run_query(cx));
                }),
            )
        })
    }

    fn query_editor_tools(&self, compact: bool, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let favorite_keys = self.action_shortcut("save-favorite");
        let suggest_keys = Some(bookie_gpui::keymap::completion_chord().label());
        let label = |id| bookie_gpui::palette::menu_label(id).unwrap_or(id);
        let (explain_label, favorite_label) = (label("explain"), label("save-favorite"));
        let tools = crate::surfaces::menu_button(
            "sql-editor-tools",
            crate::surfaces::MenuTrigger::new("SQL editor tools", Bar::Small)
                .icon("sliders-horizontal")
                .label((!compact).then_some("Tools")),
            theme,
        );
        tools.dropdown_menu(move |mut menu, _, cx| {
            let explain = owner.clone();
            let format = owner.clone();
            let favorite = owner.clone();
            menu = menu
                .item(
                    crate::menus::item(explain_label, None, None, false).on_click(move |_, _, cx| {
                        let _ = explain.update(cx, |view, cx| {
                            view.explain_query(cx);
                            cx.notify();
                        });
                    }),
                )
                .item(
                    crate::menus::item("Format SQL", None, None, false).on_click(move |_, window, cx| {
                        let _ = format.update(cx, |view, cx| view.format_query(window, cx));
                    }),
                )
                .item(
                    crate::menus::item(favorite_label, None, favorite_keys.clone(), false).on_click(move |_, _, cx| {
                        let _ = favorite.update(cx, |view, cx| view.save_favorite(cx));
                    }),
                )
                .separator();
            for (label, replace) in [("Find in editor", false), ("Replace in editor", true)] {
                let owner = owner.clone();
                menu = menu.item(crate::menus::item(label, None, None, false).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.sql_input.update(cx, |editor, cx| editor.open_search(replace, cx));
                    });
                }));
            }
            let complete = owner.clone();
            menu = menu.item(
                crate::menus::item("Suggest SQL", None, suggest_keys.clone(), false).on_click(move |_, window, cx| {
                    let _ = complete.update(cx, |view, cx| {
                        view.sql_input
                            .update(cx, |editor, cx| editor.show_completions(window, cx));
                    });
                }),
            );
            let snippets = owner.read_with(cx, |view, _| view.sql_snippets.clone()).ok().flatten();
            crate::sql_editor::snippet_menu(menu, &owner, snippets)
        })
    }

    fn query_connection_chip(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let name = self.catalog.connection.clone();
        let tooltip = connection_chip_tooltip(&name);
        picker_chip("query-connection-chip", Some("database"), name, tooltip, theme).on_click(cx.listener(
            |this, _, _, cx| {
                this.show_status();
                cx.notify();
            },
        ))
    }

    fn query_context_menu(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        let connection = if self.catalog.connection.is_empty() {
            "No active connection".to_owned()
        } else {
            format!("Connection: {}", self.catalog.connection)
        };
        let schema = self
            .supports_schema_context()
            .then(|| self.schema_context.clone().unwrap_or_else(|| "schema".to_owned()));
        crate::surfaces::menu_button(
            "query-context-menu",
            crate::surfaces::MenuTrigger::new("Connection and database context", Bar::Small).icon("database"),
            theme,
        )
        .dropdown_menu(move |menu, _, _| {
            let connection_owner = owner.clone();
            let mut menu = menu.item(crate::menus::item(connection.clone(), None, None, false).on_click(
                move |_, _, cx| {
                    let _ = connection_owner.update(cx, |view, cx| {
                        view.show_status();
                        cx.notify();
                    });
                },
            ));
            if let Some(schema) = schema.clone() {
                let schema_owner = owner.clone();
                menu = menu.item(
                    crate::menus::item(format!("Database: {schema}…"), None, None, false).on_click(
                        move |_, window, cx| {
                            let _ = schema_owner.update(cx, |view, cx| {
                                view.open_picker(crate::palette_state::PaletteMode::SchemaContext, window, cx);
                                cx.notify();
                            });
                        },
                    ),
                );
            }
            menu
        })
    }

    fn schema_context_chip(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !self.supports_schema_context() {
            return None;
        }
        let label = self.schema_context.clone().unwrap_or_else(|| "schema".to_owned());
        Some(
            picker_chip(
                "query-schema-chip",
                None,
                label,
                "Choose the schema or database queries run in",
                theme,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_picker(crate::palette_state::PaletteMode::SchemaContext, window, cx);
                cx.notify();
            })),
        )
    }

    pub(crate) fn query_editor_pane(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
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
            .child(self.query_editor_toolbar(theme, compact, cx))
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
                            .min_w_0()
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

    pub fn query_page(&mut self, theme: Theme, grid: Div, compact: bool, cx: &mut Context<Self>) -> Div {
        let editor = self.query_editor_pane(theme, compact, cx);
        let divider = self.query_split_divider(theme, cx);
        let result = self.result_pane(theme, grid, compact, cx);
        let tools = self.value_search_bar(theme, cx);
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
    ) -> crate::surfaces::Button {
        let tab = segment(("run", index), run.label(), active, theme).flex_none().child(
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
        let tab = tab.when_some(run.tooltip(), |tab, text| tab.tooltip(text));
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
    ) -> crate::surfaces::Button {
        segment(id, label, self.panel.view == view, theme).on_click(cx.listener(move |this, _, _, cx| {
            this.select_result_view(view, cx);
        }))
    }

    fn select_result_view(&mut self, view: ResultView, cx: &mut Context<Self>) {
        if view == ResultView::Chart {
            self.show_chart(cx);
        }
        self.panel.view = view;
        cx.notify();
    }

    fn result_view_menu(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.panel.view;
        let mut views: Vec<(gpui::SharedString, ResultView)> = vec![
            ("Table data".into(), ResultView::Table),
            ("Summary".into(), ResultView::Summary),
            ("Messages".into(), ResultView::Messages),
        ];
        if !self.panel.plan.is_empty() {
            views.push(("Plan".into(), ResultView::Plan));
        }
        if !self.run_tabs.slots.is_empty() {
            views.push(("Chart".into(), ResultView::Chart));
        }
        let current = views
            .iter()
            .find(|(_, view)| *view == selected)
            .map_or_else(|| "Table data".to_string(), |(label, _)| label.to_string());
        let owner = cx.entity().downgrade();
        select_button("result-view-menu", Some(&current), "Result view", Bar::Compact, theme).dropdown_menu(
            move |menu, _, _| {
                let owner = owner.clone();
                crate::menus::select(menu, &views, Some(&selected), move |view, _, cx| {
                    let _ = owner.update(cx, |this, cx| this.select_result_view(view, cx));
                })
            },
        )
    }

    pub(crate) fn result_header(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
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
                .flex_1()
                .min_w_0()
                .when(empty, |runs| runs.p_0()),
            )
            .when(compact, |header| header.child(self.result_view_menu(theme, cx)))
            .when(!compact, |header| header.child(segmented(views, theme)))
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
                div()
                    .id("execution-summary-elapsed")
                    .child(line)
                    .when_some(run.tooltip(), |el, text| el.tooltip(tip(text)))
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
                .child(list_state(ListState::Empty("Run a query to see results"), theme))
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

    pub(crate) fn result_pane(&mut self, theme: Theme, grid: Div, compact: bool, cx: &mut Context<Self>) -> Div {
        let header = self.result_header(theme, compact, cx);
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

fn connection_chip_tooltip(name: &str) -> String {
    if name.is_empty() {
        "Show connection and tunnel status".to_owned()
    } else {
        format!("{name}: show connection and tunnel status")
    }
}

fn shortcut_hint(label: &'static str, keys: String, theme: Theme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .w(px(size::SHORTCUT_HINT_WIDTH))
        .text_size(px(size::TEXT_SMALL))
        .child(truncated(format!("shortcut-hint-{label}"), label).flex_1())
        .children(shortcut(Some(keys), theme).map(|keycap| keycap.w(px(size::SHORTCUT_KEYCAP_WIDTH))))
}

#[cfg(test)]
mod connection_chip_tests {
    #[test]
    fn the_connection_chip_tooltip_names_the_connection() {
        assert!(super::connection_chip_tooltip("shop").starts_with("shop"));
        assert_eq!(super::connection_chip_tooltip(""), "Show connection and tunnel status");
    }
}
