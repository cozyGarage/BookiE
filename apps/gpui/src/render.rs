use super::*;
use crate::chrome::{STATUS_HEIGHT, TAB_HEIGHT, TOOLBAR_HEIGHT};

impl GridView {
    fn sync_render_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.keystroke_hook.is_none() {
            let view = cx.entity();
            self.keystroke_hook = Some(cx.intercept_keystrokes(move |event, window, cx| {
                view.update(cx, |this, cx| this.intercept_key(event, window, cx));
            }));
        }
        if self.pager.total != self.dataset_count() {
            self.pager = self.pager.with_total(self.dataset_count());
        }
        if std::mem::take(&mut self.find_reset) {
            self.find.update(cx, |find, cx| find.set_content("", cx));
        }
        if std::mem::take(&mut self.focus_editor) {
            let editor = self.sql_input.read(cx).focus_handle(cx);
            window.focus(&editor, cx);
        }
        self.note_frame();
        self.pace_frame(window);
        self.viewport_rows = self.viewport_rows_for(window);
        let sidebar_width = if self.sidebar.open { self.sidebar.width } else { 0.0 };
        let grid_width = (f32::from(window.viewport_size().width) - sidebar_width).max(0.0);
        self.sync_table(grid_width, cx);
    }

    fn grid_pager_buttons(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
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
        let pager = div().flex().items_center().gap_1().child(div().child(progress));
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
            .child(
                button("page-size", format!("{} rows", self.pager.rows), theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        let index = PAGE_SIZES.iter().position(|size| *size == this.pager.rows).unwrap_or(0);
                        this.pager = this.pager.resized(PAGE_SIZES[(index + 1) % PAGE_SIZES.len()]);
                        cx.notify();
                    },
                )),
            )
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
            .items_center()
            .gap_1()
            .child(div().w(px(size::FIND_WIDTH)).flex_none().child(self.find.clone()))
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

    fn grid_control_strip(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_wrap()
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
        self.filter_visible.then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .pb_2()
                .child(div().child("WHERE"))
                .child(div().w(px(size::FILTER_WIDTH)).child(self.filter.clone()))
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

    fn grid_pane(&self, theme: Theme, strip: Div, filter_row: Option<Div>, compare_bar: Option<Div>) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .child(strip)
            .children(filter_row)
            .children(compare_bar)
            .child(self.data_table())
            .children(self.detail_panel(theme))
    }

    fn main_layout(&self, sidebar: Option<Div>, tab_row: Stateful<Div>, content: Div) -> Div {
        div()
            .flex_1()
            .flex()
            .min_h_0()
            .children(sidebar)
            .child(div().flex_1().flex().flex_col().min_w_0().child(tab_row).child(content))
    }
}

impl Render for GridView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_render_state(window, cx);
        let theme = Theme::current(cx);
        let strip = self.grid_control_strip(theme, cx);
        let filter_row = self.grid_filter_row(theme, cx);
        let compare_bar = self.grid_compare_bar(theme, cx);
        let palette = self.palette.open.then(|| self.palette_overlay(cx));
        let grid = self.grid_pane(theme, strip, filter_row, compare_bar);
        let content = match self.tabs.current().clone() {
            Tab::Home => self.home_page(theme, cx),
            Tab::Settings => self.settings_page(theme, cx),
            Tab::Query => self.query_page(theme, grid, cx),
            Tab::Table(name) if name == self.catalog.grid_table => grid,
            Tab::Table(table) => self.placeholder_page(&table_label(&table), theme),
        };
        let sidebar = self.sidebar.open.then(|| self.sidebar(theme, cx));
        let toolbar = self.toolbar(theme, cx);
        let tab_row = self.tab_bar(theme, cx);
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_size(px(self.fonts.settings.ui_size))
            .when(!self.fonts.current.is_empty(), |root| root.font_family(self.fonts.current.clone()))
            .when(!self.fonts.settings.ui_family.is_empty(), |root| {
                root.font_family(self.fonts.settings.ui_family.clone())
            })
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                if this.sidebar.resizing {
                    this.sidebar.width = f32::from(event.position.x).clamp(260.0, 480.0);
                    cx.notify();
                }
                if this.panel.resizing {
                    let top_offset = TOOLBAR_HEIGHT + TAB_HEIGHT;
                    let height = f32::from(window.viewport_size().height) - top_offset - STATUS_HEIGHT;
                    this.panel.ratio = result_ratio_from_drag(f32::from(event.position.y), top_offset, height);
                    cx.notify();
                }
            }))
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
            .child(self.main_layout(sidebar, tab_row, content))
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
    fn palette_overlay(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let actions = self.palette_actions();
        let shown = palette_matches(self.palette.input.read(cx).content(), &actions);
        let selected = self.palette.index.min(shown.len().saturating_sub(1));
        let list = shown.iter().enumerate().map(|(position, index)| {
            let row = div().px_2().py_1().rounded_md().child(actions[*index].label.clone());
            if position == selected {
                row.bg(rgb(theme.active_row))
            } else {
                row
            }
        });
        let empty = shown.is_empty().then(|| {
            div()
                .px_2()
                .text_color(rgb(theme.muted_foreground))
                .child("No matching command")
        });
        div()
            .absolute()
            .top(px(size::PALETTE_TOP))
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(size::PALETTE_WIDTH))
                    .p_2()
                    .gap_1()
                    .flex()
                    .flex_col()
                    .rounded(px(size::SURFACE_RADIUS))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.background))
                    .shadow_lg()
                    .child(self.palette.input.clone())
                    .children(list)
                    .children(empty),
            )
    }
}
