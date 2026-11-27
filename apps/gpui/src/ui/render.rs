use crate::chrome::{STATUS_HEIGHT, TAB_HEIGHT, TOOLBAR_HEIGHT};
use crate::palette_state::PaletteMode;
use crate::surfaces::{
    Bar, ButtonKind, button, compact_button, compact_icon_button, dialog_surface, scrim, segment, segmented,
};
use crate::*;
use gpui::uniform_list;

impl GridView {
    fn sync_render_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tabs.current();
        window.set_window_title(&format!("{} — BookiE", tab.title()));
        self.window_title_tab = Some(tab.clone());
        if self.keystroke_hook.is_none() {
            let view = cx.entity();
            self.keystroke_hook = Some(cx.intercept_keystrokes(move |event, window, cx| {
                view.update(cx, |this, cx| this.intercept_key(event, window, cx));
            }));
        }
        if self.pager.total != self.dataset_count() {
            self.pager = self.pager.with_total(self.dataset_count());
        }
        if std::mem::take(&mut self.search.reset) {
            self.search.find.update(cx, |find, cx| find.set_content("", cx));
        }
        let placeholder = crate::editor_placeholder(&self.keymap);
        if self.editor_hint != placeholder {
            self.editor_hint = placeholder.clone();
            self.sql_input
                .update(cx, |editor, cx| editor.set_placeholder(placeholder, window, cx));
        }
        if std::mem::take(&mut self.focus_editor) {
            let editor = self.sql_input.read(cx).focus_handle(cx);
            window.focus(&editor, cx);
        }
        self.prepare_tools(window, cx);
        self.note_frame();
        self.pace_frame(window);
        self.viewport_rows = self.viewport_rows_for(window);
        self.sidebar
            .collapse_when_narrow(f32::from(window.viewport_size().width));
        let sidebar_width = if self.sidebar.open { self.sidebar.width } else { 0.0 };
        let content_width = (f32::from(window.viewport_size().width) - sidebar_width).max(0.0);
        let grid_width = (content_width - self.detail_sidebar_width(content_width)).max(0.0);
        self.sync_table(window, cx);
        if self.synced_width != grid_width {
            self.synced_width = grid_width;
            let table = self.table.clone();
            window.on_next_frame(move |_, cx| table.update(cx, |_, cx| cx.notify()));
        }
    }

    fn drag_resize(&mut self, event: &gpui::MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.current() == &Tab::Diagram {
            self.drag_diagram_card(event, cx);
        }
        if self.sidebar.resizing {
            self.sidebar.width = f32::from(event.position.x).clamp(260.0, 480.0);
            cx.notify();
        }
        if self.panel.resizing {
            let top_offset = TOOLBAR_HEIGHT + TAB_HEIGHT;
            let height = f32::from(window.viewport_size().height) - top_offset - STATUS_HEIGHT;
            self.panel.ratio = result_ratio_from_drag(f32::from(event.position.y), top_offset, height);
            cx.notify();
        }
    }

    fn row_menu_overlay(&self) -> Option<impl IntoElement> {
        self.row_menu.as_ref().map(|row_menu| {
            gpui::deferred(
                gpui::anchored()
                    .position(row_menu.position)
                    .snap_to_window_with_margin(px(size::SM))
                    .child(row_menu.menu.clone()),
            )
        })
    }

    fn page_options(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::DropdownMenu as _;

        let owner = cx.entity().downgrade();
        let rows = self.pager.rows;
        let continuous = self.continuous;
        let groups = crate::menus::command_groups(self, crate::menus::PAGE_GROUPS);
        let modes = crate::menus::command_groups(self, &[&["continuous", "append-pages"]]);
        let sizes: Vec<(gpui::SharedString, u64)> = PAGE_SIZES
            .into_iter()
            .map(|size| (format!("{} rows per page", thousands(size)).into(), size))
            .collect();
        let label = if continuous {
            "Continuous".to_string()
        } else {
            format!("{} / page", thousands(rows))
        };
        crate::surfaces::select_button(
            "page-size",
            Some(&label),
            "Rows per page and navigation mode",
            Bar::Compact,
            theme,
        )
        .dropdown_menu_with_anchor(gpui::Anchor::BottomRight, move |menu, _, _| {
            let pick = owner.clone();
            let menu = crate::menus::select(menu, &sizes, (!continuous).then_some(&rows), move |size, _, cx| {
                let _ = pick.update(cx, |this, cx| {
                    this.pager = this.pager.resized(size);
                    this.continuous = false;
                    cx.notify();
                });
            });
            let menu = crate::menus::fill(menu.separator(), &owner, &modes);
            if continuous || groups.is_empty() {
                return menu;
            }
            crate::menus::fill(menu.separator(), &owner, &groups)
        })
    }

    fn grid_pager(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let count = self.dataset_count();
        let range = (!self.continuous && self.pager.pages() > 1).then(|| self.pager.range());
        let label = page_summary(count, self.source.is_partial(), range);
        let can_page = range.is_some();
        let previous = compact_icon_button("pager-prev", "chevron-left", "Previous page", theme).on_click(cx.listener(
            |this, _, _, cx| {
                this.pager.previous();
                cx.notify();
            },
        ));
        let next =
            compact_icon_button("next", "chevron-right", "Next page", theme).on_click(cx.listener(|this, _, _, cx| {
                this.pager.next();
                cx.notify();
            }));
        div()
            .debug_selector(|| "grid-pager".into())
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap_1()
            .px_2()
            .py_1()
            .border_t_1()
            .border_color(rgb(theme.border))
            .text_color(rgb(theme.muted_foreground))
            .children(can_page.then_some(previous))
            .child(
                div()
                    .debug_selector(|| "page-summary".into())
                    .min_w_0()
                    .truncate()
                    .id("page-summary")
                    .tooltip(crate::surfaces::tooltip_view(label.clone().into()))
                    .child(label),
            )
            .children(can_page.then_some(next))
            .child(self.page_options(theme, cx))
    }

    fn grid_icon(
        &self,
        id: &'static str,
        name: &'static str,
        tooltip: impl Into<gpui::SharedString>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        compact_icon_button(id, name, tooltip, theme).on_click(cx.listener(move |this, _, window, cx| {
            this.run_palette_id(id, window, cx);
            cx.notify();
        }))
    }

    fn grid_command_tip(&self, id: &str) -> String {
        let label = bookie_gpui::palette::menu_label(id).unwrap_or(id);
        bookie_gpui::keymap::Command::for_action(id)
            .map_or_else(|| label.to_string(), |command| self.labelled_tip(label, command))
    }

    fn add_row_button(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        (self.can_save_edits() && self.cell_edit.edits.is_empty() && self.compare.is_none() && !self.form_open())
            .then(|| self.grid_icon("insert-row", "plus", self.grid_command_tip("insert-row"), theme, cx))
    }

    fn export_menu_button(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::DropdownMenu;
        let groups = crate::menus::command_groups(self, crate::menus::EXPORT_GROUPS);
        crate::surfaces::icon_menu_button("grid-export", "download", None, "Export", Bar::Compact, theme)
            .dropdown_menu(crate::menus::command_menu(cx.entity().downgrade(), groups))
    }

    fn grid_more_menu(&self, hidden: usize, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::DropdownMenu;
        let mut groups = crate::menus::command_groups(self, crate::menus::GRID_MORE_GROUPS);
        crate::menus::set_enabled(&mut groups, "show-hidden-columns", hidden > 0);
        crate::menus::set_enabled(&mut groups, "compare", self.live.is_none());
        crate::surfaces::icon_menu_button("grid-more", "ellipsis", None, "More grid actions", Bar::Compact, theme)
            .dropdown_menu(crate::menus::command_menu(cx.entity().downgrade(), groups))
    }

    fn grid_wide_actions(&self, hidden: usize, theme: Theme, cx: &mut Context<Self>) -> Div {
        use gpui_component::menu::DropdownMenu;
        let columns = crate::menus::command_groups(self, crate::menus::COLUMN_GROUPS);
        div()
            .flex()
            .items_center()
            .gap_1()
            .children(
                (self.live.is_some() && !self.grid_is_query)
                    .then(|| self.grid_icon("refresh-table", "refresh-cw", "Refresh from the database", theme, cx)),
            )
            .children(
                (hidden > 0)
                    .then(|| self.grid_icon("show-hidden-columns", "eye", show_all_columns_label(hidden), theme, cx)),
            )
            .children(self.add_row_button(theme, cx))
            .child(
                crate::surfaces::icon_menu_button(
                    "grid-columns-menu",
                    "columns-3",
                    None,
                    "Columns",
                    Bar::Compact,
                    theme,
                )
                .dropdown_menu(crate::menus::command_menu(cx.entity().downgrade(), columns)),
            )
            .children(
                self.live
                    .is_none()
                    .then(|| self.grid_icon("compare", "git-compare", "Compare results", theme, cx)),
            )
            .child(self.export_menu_button(theme, cx))
    }

    fn grid_action_buttons(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let compact = self.synced_width < 1000.0;
        let hidden = self.table.read(cx).delegate().hidden_column_count();
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .debug_selector(|| "grid-actions".into())
            .items_center()
            .gap_1()
            .child(
                div()
                    .flex_1()
                    .min_w(px(size::FIND_MIN_WIDTH))
                    .max_w(px(size::FIND_WIDTH))
                    .debug_selector(|| "grid-find-field".into())
                    .child(self.search.find.clone()),
            )
            .child(
                compact_icon_button("find-prev", "chevron-up", "Previous match", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.run_find(false, cx))),
            )
            .child(
                compact_icon_button("find-next", "chevron-down", "Next match", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.run_find(true, cx))),
            )
            .child(
                compact_icon_button("show", "sliders-horizontal", "Show matching rows only", theme).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.show_matching(cx);
                        cx.notify();
                    }),
                ),
            )
            .child(self.loaded_filter_toggle(theme, cx).ml_auto())
            .children(self.pending_marker(theme, cx))
            .when(!compact, |actions| {
                actions.child(self.grid_wide_actions(hidden, theme, cx))
            })
            .when(compact, |actions| actions.child(self.grid_more_menu(hidden, theme, cx)))
    }

    fn loaded_filter_toggle(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        crate::surfaces::toggle_icon_button(
            "toggle-loaded-filter",
            "funnel",
            self.grid_command_tip("filter"),
            self.search.filter_visible,
            Bar::Compact,
            theme,
        )
        .on_click(cx.listener(|this, _, window, cx| {
            if this.search.filter_visible {
                this.search.filter_visible = false;
                window.focus(&this.focus, cx);
            } else {
                this.run_palette_id("filter", window, cx);
            }
            cx.notify();
        }))
    }

    fn grid_control_strip(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("grid-controls")
            .flex()
            .min_w_0()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .children(self.where_toggle(theme, cx))
            .child(self.grid_action_buttons(theme, cx))
    }

    fn grid_filter_row(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        self.search.filter_visible.then(|| {
            div()
                .debug_selector(|| "loaded-row-filter".into())
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .child(div().flex_none().child("Filter loaded rows"))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .max_w(px(size::FILTER_WIDTH))
                        .child(self.search.filter.clone()),
                )
                .child(
                    compact_button(ButtonKind::Primary, "apply", "Apply", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.apply_filter(cx))),
                )
                .child(
                    compact_button(ButtonKind::Ghost, "clear", "Clear", theme).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.clear_filter();
                            cx.notify();
                        },
                    )),
                )
        })
    }

    fn grid_compare_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        self.compare.as_ref().map(|diff| {
            let s = &diff.summary;
            let summary = if s.keyed {
                format!(
                    "Equal {} · Removed {} · Changed {}",
                    thousands(s.equal),
                    thousands(s.removed),
                    thousands(s.changed)
                )
            } else {
                format!(
                    "Equal {} · Added {} · Removed {}",
                    thousands(s.equal),
                    thousands(s.added),
                    thousands(s.removed)
                )
            };
            let summary = if s.unknown > 0 {
                format!("{summary} · Unknown {}", thousands(s.unknown))
            } else {
                summary
            };
            let capped = if diff.capped {
                format!(" · first {MAX_DIFF_ROWS} differences shown")
            } else {
                String::new()
            };
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .pb_2()
                .child(div().child(format!("COMPARE {summary}{capped}")))
                .child(segmented(
                    [
                        segment("whole", "Whole row", !s.keyed, theme)
                            .on_click(cx.listener(|this, _, _, cx| this.compare(false, cx))),
                        segment("keyed", "Key: value", s.keyed, theme)
                            .on_click(cx.listener(|this, _, _, cx| this.compare(true, cx))),
                    ],
                    theme,
                ))
                .child(
                    button(ButtonKind::Ghost, "close-compare", "Close", theme).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.compare = None;
                            cx.notify();
                        },
                    )),
                )
        })
    }

    fn grid_pane(
        &self,
        theme: Theme,
        strip: Stateful<Div>,
        bars: (Option<Div>, Option<Div>, Option<Div>),
        cx: &mut Context<Self>,
    ) -> Div {
        let (filter_row, compare_bar, insert_form) = bars;
        let grid = div()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .child(strip)
            .children(filter_row)
            .children(compare_bar)
            .children(insert_form)
            .child(self.data_table(cx))
            .child(self.grid_pager(theme, cx));
        div()
            .flex_1()
            .flex()
            .min_h_0()
            .child(grid)
            .children(self.detail_sidebar(theme, cx))
    }

    fn main_layout(&self, sidebar: Option<Div>, tab_row: Stateful<Div>, content: Div) -> Div {
        div()
            .flex_1()
            .flex()
            .min_h_0()
            .children(sidebar)
            .child(div().flex_1().flex().flex_col().min_w_0().child(tab_row).child(content))
    }

    fn full_value_editor_overlay(&self, theme: Theme, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let editing = self.cell_edit.active.as_ref().filter(|editing| editing.full_value)?;
        let column = self
            .source
            .columns()
            .get(editing.column)
            .map_or("value", |column| column.name.as_str())
            .to_string();
        Some(
            scrim(theme)
                .id("full-value-editor-overlay")
                .occlude()
                .child(
                    dialog_surface(size::FULL_VALUE_EDITOR_WIDTH, theme)
                        .id("full-value-editor-dialog")
                        .debug_selector(|| "full-value-editor-dialog".into())
                        .max_w(gpui::relative(0.92))
                        .h(px(size::FULL_VALUE_EDITOR_HEIGHT))
                        .max_h(gpui::relative(0.9))
                        .min_h_0()
                        .p_3()
                        .gap_2()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(format!("Edit {column}")),
                                )
                                .child(div().text_color(rgb(theme.muted_foreground)).child("Full value")),
                        )
                        .child(
                            div()
                                .id("full-value-editor-input")
                                .flex_1()
                                .min_h_0()
                                .border_1()
                                .border_color(rgb(theme.border))
                                .child(
                                    gpui_kit::component::input::Editor::new(&self.cell_edit.value_editor)
                                        .h(gpui::relative(1.0)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .min_w_0()
                                        .text_color(rgb(theme.muted_foreground))
                                        .child(self.status.clone()),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .debug_selector(|| "full-value-actions".into())
                                        .child(
                                            button(ButtonKind::Primary, "apply-full-value", "Apply", theme).on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.apply_active_cell_edit(window, cx)
                                                }),
                                            ),
                                        )
                                        .child(
                                            button(ButtonKind::Ghost, "cancel-full-value", "Cancel", theme).on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.cancel_active_cell_edit(window, cx)
                                                }),
                                            ),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for GridView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_row_inspector(cx);
        self.sync_render_state(window, cx);
        if std::mem::take(&mut self.connection_form.focus_on_open) {
            let name = gpui::Focusable::focus_handle(self.connection_form.name.read(cx), cx);
            window.focus(&name, cx);
        }
        if std::mem::take(&mut self.connection_form.restore_focus) {
            window.focus(&self.focus, cx);
        }
        if let Some(focus) = self.connection_form.focus_validation.take() {
            window.focus(&focus, cx);
        }
        if self.approval.current.is_some() && !self.focus.is_focused(window) {
            window.focus(&self.focus, cx);
        }
        let theme = Theme::current(cx);
        let strip = self.grid_control_strip(theme, cx);
        let filter_row = Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .children(self.server_filter_row(theme, cx))
                .children(self.grid_filter_row(theme, cx)),
        );
        let compare_bar = self.grid_compare_bar(theme, cx);
        let palette = if self.palette.open {
            Some(self.palette_overlay(window, cx))
        } else {
            None
        };
        let full_value_editor = self.full_value_editor_overlay(theme, cx);
        let approval = self.approval_overlay(theme, cx);
        let host_key = self.host_key_overlay(theme, cx);
        let close_confirm = self.close_confirm_overlay(theme, cx);
        let cancel_process = self.cancel_process_overlay(theme, cx);
        let connection_dialog = self.connection_form.open.then(|| self.connection_dialog(theme, cx));
        let bar_dialog = self
            .import_bar(theme, cx)
            .or_else(|| self.export_file_bar(theme, cx))
            .or_else(|| self.schema_diagram_bar(theme, cx))
            .or_else(|| self.save_bar(theme, cx))
            .or_else(|| self.insert_form(theme, cx))
            .or_else(|| self.data_compare_bar(theme, cx))
            .or_else(|| self.one_shot_bar(theme, cx))
            .map(|bar| bar_dialog(theme, bar));
        let insert_form = self
            .structure_info_panel(theme, cx)
            .or_else(|| self.late_bars(theme, cx));
        let grid = self.grid_pane(theme, strip, (filter_row, compare_bar, insert_form), cx);
        let content_width =
            f32::from(window.viewport_size().width) - if self.sidebar.open { self.sidebar.width } else { 0.0 };
        let compact_query = content_width < size::COMPACT_QUERY_WIDTH;
        let content = match self.tabs.current().clone() {
            Tab::Home => self.home_page(theme, cx),
            Tab::Settings => self.settings_page(theme, cx),
            Tab::History => self.history_page(theme, cx),
            Tab::AuditLog => self.audit_page(theme, cx),
            Tab::Diagram => self.diagram_page(theme, cx),
            Tab::Query => self.query_page(theme, grid, compact_query, cx),
            Tab::Table(name) if name == self.catalog.grid_table => grid,
            Tab::Table(table) => self.placeholder_page(&table_label(&table), theme),
        };
        let sidebar = self.sidebar.open.then(|| self.sidebar(theme, cx));
        let toolbar = self.toolbar(theme, window, cx);
        let tab_row = self.tab_bar(theme, cx);
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_size(px(self.fonts.settings.ui_size))
            .when(!self.fonts.current.is_empty(), |root| {
                root.font_family(self.fonts.current.clone())
            })
            .when(!self.fonts.settings.ui_family.is_empty(), |root| {
                root.font_family(self.fonts.settings.ui_family.clone())
            })
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(cx.listener(Self::drag_resize))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseUpEvent, _, cx| {
                    this.end_diagram_gesture();
                    let was_resizing =
                        std::mem::take(&mut this.sidebar.resizing) | std::mem::take(&mut this.panel.resizing);
                    if was_resizing {
                        cx.notify();
                    }
                }),
            )
            .child(toolbar)
            .child(self.main_layout(sidebar, tab_row, content))
            .children(self.row_menu_overlay())
            .child(self.status_bar(theme, f32::from(window.viewport_size().width), cx))
            .children(palette)
            .children(full_value_editor)
            .children(bar_dialog)
            .children(self.notifications_list(theme, cx))
            .children(connection_dialog)
            .children(self.delete_connection_overlay(theme, cx))
            .children(approval)
            .children(host_key)
            .children(close_confirm)
            .children(cancel_process)
    }
}

fn bar_dialog(theme: Theme, bar: Div) -> Stateful<Div> {
    crate::surfaces::scrim(theme).id("bar-dialog-scrim").occlude().child(
        crate::surfaces::dialog_surface(size::FULL_VALUE_EDITOR_WIDTH, theme)
            .id("bar-dialog")
            .debug_selector(|| "bar-dialog".into())
            .max_h(gpui::relative(0.9))
            .overflow_y_scroll()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(bar),
    )
}

pub(crate) fn page_summary(rows: u64, partial: bool, range: Option<(u64, u64)>) -> String {
    let count = thousands(rows);
    match (range, partial) {
        (Some((first, last)), true) => format!("Rows {}–{} of ≥ {count}", thousands(first), thousands(last)),
        (Some((first, last)), false) => format!("Rows {}–{} of {count}", thousands(first), thousands(last)),
        (None, true) => format!("≥ {count} rows"),
        (None, false) => format!("{count} rows"),
    }
}

impl GridView {
    pub(crate) fn action_shortcut(&self, id: &str) -> Option<String> {
        if id == "go-to-last-edit" {
            return Some(bookie_gpui::keymap::last_edit_chord().label());
        }
        bookie_gpui::keymap::Command::for_action(id)
            .and_then(|command| self.keymap.binding(command))
            .map(bookie_gpui::keymap::Chord::label)
    }

    pub(crate) fn command_tip(&self, command: bookie_gpui::keymap::Command) -> String {
        bookie_gpui::keymap::with_shortcut(command.title(), self.keymap.binding(command))
    }

    fn command_rows(
        &self,
        actions: &[Action],
        shown: &[usize],
        range: std::ops::Range<usize>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        let selected = self.palette.index.min(shown.len().saturating_sub(1));
        shown[range.clone()]
            .iter()
            .zip(range)
            .map(|(index, position)| {
                let action = &actions[*index];
                let shortcut = self.action_shortcut(action.id);
                let row = div()
                    .id(("palette-command-row", position))
                    .debug_selector(move || format!("palette-command-row-{position}").into())
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .cursor_pointer()
                    .child(action.label.clone())
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .text_color(rgb(theme.muted_foreground))
                            .child(bookie_gpui::palette::group(action.id))
                            .children(shortcut.map(|keys| div().child(keys))),
                    );
                let hover = theme.active_row;
                let row =
                    row.hover(move |style| style.bg(rgb(hover)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.palette.index = position;
                            this.run_palette(window, cx);
                        }));
                if position == selected {
                    row.bg(rgb(theme.active_row))
                } else {
                    row
                }
            })
            .collect()
    }

    fn picker_rows(
        &self,
        shown: &[crate::picker::PickerItem],
        range: std::ops::Range<usize>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        let selected = self.palette.index.min(shown.len().saturating_sub(1));
        shown[range.clone()]
            .iter()
            .zip(range)
            .map(|(item, position)| {
                let mut row = div()
                    .id(("palette-picker-row", position))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(item.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.muted_foreground))
                            .child(item.subtitle.clone()),
                    );
                let hover = theme.active_row;
                row = row
                    .hover(move |style| style.bg(rgb(hover)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.palette.index = position;
                        this.run_picker(window, cx);
                    }));
                if position == selected {
                    row.bg(rgb(theme.active_row))
                } else {
                    row
                }
            })
            .collect()
    }

    fn palette_overlay(&self, window: &Window, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let theme = Theme::current(cx);
        let empty_text = match self.palette.mode {
            PaletteMode::Commands | PaletteMode::Shortcuts => "No matching command",
            PaletteMode::Tables => "No matching table",
            PaletteMode::Columns => "No matching column",
            PaletteMode::ChartValues => "No matching numeric column",
            PaletteMode::Favorites | PaletteMode::DeleteFavorite => "No saved favorites",
            PaletteMode::SavedFilters | PaletteMode::DeleteSavedFilter => "No saved filters for this table",
            PaletteMode::NameFilter => "Type a name for the filter",
            PaletteMode::SchemaContext => "Enter a valid schema or database name",
            PaletteMode::Themes => "No matching theme",
            PaletteMode::CompareTarget => "No other table to compare with",
        };
        let (count, list) = if self.palette.mode == PaletteMode::Commands {
            let actions = self.palette_actions();
            let shown = palette_matches(self.palette.input.read(cx).content(), &actions);
            let count = shown.len();
            let rows = cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let theme = Theme::current(cx);
                this.command_rows(&actions, &shown, range, &theme, cx)
            });
            (count, uniform_list("palette-results", count, rows))
        } else {
            let shown = self.picker_items(cx);
            let count = shown.len();
            let rows = cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let theme = Theme::current(cx);
                this.picker_rows(&shown, range, &theme, cx)
            });
            (count, uniform_list("palette-results", count, rows))
        };
        let empty = (count == 0).then(|| div().px_2().text_color(rgb(theme.muted_foreground)).child(empty_text));
        let max_height = (window.viewport_size().height - px(size::PALETTE_TOP * 1.5 + STATUS_HEIGHT)).max(px(0.0));
        scrim(theme)
            .id("palette-overlay")
            .items_start()
            .occlude()
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_palette(window, cx);
                cx.stop_propagation();
            }))
            .child(
                dialog_surface(size::PALETTE_WIDTH, theme)
                    .id("palette-surface")
                    .debug_selector(|| "palette-surface".into())
                    .mt(px(size::PALETTE_TOP))
                    .max_h(max_height)
                    .overflow_hidden()
                    .p_2()
                    .gap_1()
                    .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                    .child(self.palette.input.clone())
                    .child(
                        list.with_sizing_behavior(gpui::ListSizingBehavior::Infer)
                            .min_h_0()
                            .track_scroll(&self.palette.scroll),
                    )
                    .children(empty),
            )
    }
}

fn show_all_columns_label(hidden: usize) -> String {
    format!("Show hidden columns ({hidden})")
}

#[cfg(test)]
mod show_all_columns_tests {
    #[test]
    fn the_hidden_columns_button_is_not_a_second_columns_control() {
        let label = super::show_all_columns_label(2);
        assert!(label.contains('2'));
        assert!(!label.starts_with("Columns"));
    }
}
