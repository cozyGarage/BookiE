use super::*;
use crate::chrome::{STATUS_HEIGHT, TAB_HEIGHT, TOOLBAR_HEIGHT};
use crate::palette_state::PaletteMode;
use gpui_kit::component::scroll::Scrollbar;

impl GridView {
    fn sync_render_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tabs.current();
        window.set_window_title(&format!("{} — BookiE", self.tab_title(tab, cx)));
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
        let grid_width = (f32::from(window.viewport_size().width) - sidebar_width).max(0.0);
        self.sync_table((grid_width - f32::from(Scrollbar::width())).max(0.0), window, cx);
        if self.synced_width != grid_width {
            self.synced_width = grid_width;
            let table = self.table.clone();
            window.on_next_frame(move |_, cx| table.update(cx, |_, cx| cx.notify()));
        }
    }

    fn pager_labels(&self) -> (String, String) {
        let (first, last) = self.pager.range();
        let title = match &self.mode {
            Mode::Filtered { label, .. } => format!("{label} · "),
            Mode::All => String::new(),
        };
        let progress = progress_text(
            &title,
            self.dataset_count(),
            self.source.is_partial(),
            self.pager.pages(),
        );
        let range_label = if self.dataset_count() == 0 {
            "Rows 0".to_string()
        } else if self.continuous {
            format!("Rows 1-{}", thousands(self.dataset_count()))
        } else {
            format!("Rows {}-{}", thousands(first), thousands(last))
        };
        (progress, range_label)
    }

    fn drag_resize(&mut self, event: &gpui::MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
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

    fn page_size_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        button("page-size", format!("{} rows", self.pager.rows), theme).on_click(cx.listener(|this, _, _, cx| {
            let index = PAGE_SIZES.iter().position(|size| *size == this.pager.rows).unwrap_or(0);
            this.pager = this.pager.resized(PAGE_SIZES[(index + 1) % PAGE_SIZES.len()]);
            cx.notify();
        }))
    }

    fn grid_pager_buttons(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let (progress, range_label) = self.pager_labels();
        let pager = div()
            .flex()
            .flex_none()
            .debug_selector(|| "grid-pager".into())
            .items_center()
            .gap_1()
            .child(div().child(progress));
        if self.continuous {
            return pager.child(div().child(range_label));
        }
        pager
            .child(
                icon_button("first", "chevrons-left", "First page", false, theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.pager.first();
                        cx.notify();
                    },
                )),
            )
            .child(
                icon_button("prev", "chevron-left", "Previous page", false, theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.pager.previous();
                        cx.notify();
                    },
                )),
            )
            .child(div().child(range_label))
            .child(self.page_size_button(theme, cx))
            .child(
                icon_button(
                    "load-more",
                    "arrow-down-up",
                    "Append the next page instead of replacing the current one",
                    self.pager.append,
                    theme,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pager.append = !this.pager.append;
                    cx.notify();
                })),
            )
            .child(
                icon_button("next", "chevron-right", "Next page", false, theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.pager.next();
                        cx.notify();
                    },
                )),
            )
            .child(
                icon_button("last", "chevrons-right", "Last page", false, theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.pager.last();
                        cx.notify();
                    },
                )),
            )
    }

    fn grid_action_buttons(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .debug_selector(|| "grid-actions".into())
            .items_center()
            .gap_1()
            .child(
                div()
                    .w(px(size::FIND_WIDTH))
                    .flex_none()
                    .child(self.search.find.clone()),
            )
            .child(
                icon_button("find-prev", "chevron-up", "Previous match", false, theme)
                    .on_click(cx.listener(|this, _, _, cx| this.run_find(false, cx))),
            )
            .child(
                icon_button("find-next", "chevron-down", "Next match", false, theme)
                    .on_click(cx.listener(|this, _, _, cx| this.run_find(true, cx))),
            )
            .child(
                icon_button("show", "sliders-horizontal", "Show matching rows only", false, theme).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.show_matching(cx);
                        cx.notify();
                    }),
                ),
            )
            .child(
                icon_button("compare", "arrow-left-right", "Compare", false, theme)
                    .on_click(cx.listener(|this, _, _, cx| this.compare(false, cx))),
            )
            .child(
                icon_button(
                    "export",
                    "download",
                    "Export current page or selection as CSV",
                    false,
                    theme,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.export_selection_or_page(cx);
                    cx.notify();
                })),
            )
            .child(
                icon_button(
                    "continuous",
                    "chevrons-down-up",
                    "Scroll continuously",
                    self.continuous,
                    theme,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.continuous = !this.continuous;
                    this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    cx.notify();
                })),
            )
    }

    fn grid_control_strip(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("grid-controls")
            .flex()
            .min_w_0()
            .overflow_x_scroll()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(self.grid_pager_buttons(theme, cx))
            .child(self.grid_action_buttons(theme, cx))
    }

    fn grid_filter_row(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        self.search.filter_visible.then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .pb_2()
                .child(div().child("Filter"))
                .child(div().w(px(size::FILTER_WIDTH)).child(self.search.filter.clone()))
                .child(button("apply", "Apply", theme).on_click(cx.listener(|this, _, _, cx| this.apply_filter(cx))))
                .child(
                    button("clear", "Clear all", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.clear_filter();
                        cx.notify();
                    })),
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
                .child(
                    button("whole", "Whole row", theme).on_click(cx.listener(|this, _, _, cx| this.compare(false, cx))),
                )
                .child(
                    button("keyed", "Key: value", theme).on_click(cx.listener(|this, _, _, cx| this.compare(true, cx))),
                )
                .child(
                    button("close-compare", "Close", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.compare = None;
                        cx.notify();
                    })),
                )
        })
    }

    fn grid_pane(
        &self,
        theme: Theme,
        strip: Stateful<Div>,
        bars: (Option<Div>, Option<Div>, Option<Div>, Option<Div>, Option<Div>),
        cx: &mut Context<Self>,
    ) -> Div {
        let (filter_row, compare_bar, save_bar, banner, insert_form) = bars;
        div()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .child(strip)
            .children(filter_row)
            .children(compare_bar)
            .children(save_bar)
            .children(insert_form)
            .children(banner)
            .child(self.data_table(cx))
            .children(self.detail_panel(theme))
            .children(self.row_detail_panel(theme))
            .children(self.fk_peek_panel(theme, cx))
    }

    fn main_layout(
        &self,
        sidebar: Option<Div>,
        tab_row: Stateful<Div>,
        bundle_bar: Option<Div>,
        content: Div,
    ) -> Div {
        div()
            .flex_1()
            .flex()
            .min_h_0()
            .children(sidebar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(tab_row)
                    .children(bundle_bar)
                    .child(content),
            )
    }
}

impl Render for GridView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_render_state(window, cx);
        let theme = Theme::current(cx);
        let strip = self.grid_control_strip(theme, cx);
        let filter_row = self.grid_filter_row(theme, cx);
        let compare_bar = self.grid_compare_bar(theme, cx);
        let palette = if self.palette.open {
            Some(self.palette_overlay(window, cx))
        } else {
            None
        };
        let save_bar = self
            .approval_bar(theme, cx)
            .or_else(|| self.save_bar(theme, cx))
            .or_else(|| self.edits_bar(theme, cx));
        let banner = self.save_banner(theme, cx);
        let bundle_bar = self.connection_bundle_bar(theme, cx);
        let insert_form = self
            .insert_form(theme, cx)
            .or_else(|| self.import_bar(theme, cx))
            .or_else(|| self.structure_bar(theme, cx))
            .or_else(|| self.structure_info_panel(theme, cx))
            .or_else(|| self.resolve_panel(theme, cx))
            .or_else(|| self.late_bars(theme, cx));
        let grid = self.grid_pane(
            theme,
            strip,
            (filter_row, compare_bar, save_bar, banner, insert_form),
            cx,
        );
        let content = match self.tabs.current().clone() {
            Tab::Home => self.home_page(theme, cx),
            Tab::Settings => self.settings_page(theme, cx),
            Tab::NewConnection => self.connection_page(theme, cx),
            Tab::History => self.history_page(theme, cx),
            Tab::AuditLog => self.audit_page(theme, cx),
            Tab::Diagram => self.diagram_page(theme, cx),
            Tab::Query => self.query_page(theme, grid, cx),
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
                    let was_resizing =
                        std::mem::take(&mut this.sidebar.resizing) | std::mem::take(&mut this.panel.resizing);
                    if was_resizing {
                        cx.notify();
                    }
                }),
            )
            .child(toolbar)
            .child(self.main_layout(sidebar, tab_row, bundle_bar, content))
            .children(self.row_menu_overlay())
            .child(self.status_bar(theme))
            .children(palette)
    }
}

pub(crate) fn progress_text(title: &str, rows: u64, partial: bool, pages: u64) -> String {
    let more = if partial { "+" } else { "" };
    let count = thousands(rows);
    if pages > 1 {
        format!("{title}{count}{more} rows · {pages} pages")
    } else {
        format!("{title}{count}{more} rows")
    }
}

impl GridView {
    fn command_rows(&self, theme: &Theme, cx: &mut Context<Self>) -> Vec<gpui::Stateful<Div>> {
        let actions = self.palette_actions();
        let shown = palette_matches(self.palette.input.read(cx).content(), &actions);
        let selected = self.palette.index.min(shown.len().saturating_sub(1));
        shown
            .iter()
            .enumerate()
            .map(|(position, index)| {
                let action = &actions[*index];
                let shortcut = bookie_gpui::keymap::Command::ALL
                    .into_iter()
                    .find(|command| command.name() == action.id || command.title() == action.label)
                    .and_then(|command| self.keymap.binding(command))
                    .map(bookie_gpui::keymap::Chord::label);
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

    fn picker_rows(&self, theme: &Theme, cx: &mut Context<Self>) -> Vec<gpui::Stateful<Div>> {
        let shown = self.picker_items(cx);
        let selected = self.palette.index.min(shown.len().saturating_sub(1));
        shown
            .into_iter()
            .enumerate()
            .map(|(position, item)| {
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
                            .child(item.title),
                    )
                    .child(div().text_color(rgb(theme.muted_foreground)).child(item.subtitle));
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
        let (rows, empty_text) = match self.palette.mode {
            PaletteMode::Commands => (self.command_rows(&theme, cx), "No matching command"),
            PaletteMode::Tables => (self.picker_rows(&theme, cx), "No matching table"),
            PaletteMode::Columns => (self.picker_rows(&theme, cx), "No matching column"),
            PaletteMode::Shortcuts => (self.picker_rows(&theme, cx), "No matching command"),
            PaletteMode::Favorites | PaletteMode::DeleteFavorite => {
                (self.picker_rows(&theme, cx), "No saved favorites")
            }
            PaletteMode::SavedFilters | PaletteMode::DeleteSavedFilter => {
                (self.picker_rows(&theme, cx), "No saved filters for this table")
            }
            PaletteMode::NameFilter => (self.picker_rows(&theme, cx), "Type a name for the filter"),
        };
        let empty = rows
            .is_empty()
            .then(|| div().px_2().text_color(rgb(theme.muted_foreground)).child(empty_text));
        div()
            .id("palette-overlay")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .items_start()
            .occlude()
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_palette(window, cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .id("palette-surface")
                    .w_full()
                    .max_w(px(size::PALETTE_WIDTH))
                    .mt(px(size::PALETTE_TOP))
                    .p_2()
                    .gap_1()
                    .flex()
                    .flex_col()
                    .rounded(px(size::SURFACE_RADIUS))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.background))
                    .shadow_lg()
                    .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                    .child(self.palette.input.clone())
                    .child(
                        div()
                            .id("palette-results")
                            .max_h(
                                (window.viewport_size().height - px(size::PALETTE_TOP + size::FORM_CONTROL + 40.0))
                                    .max(px(0.0)),
                            )
                            .overflow_y_scroll()
                            .track_scroll(&self.palette.scroll)
                            .children(rows)
                            .children(empty),
                    ),
            )
    }
}
