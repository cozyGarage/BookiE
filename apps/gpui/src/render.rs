use super::*;

impl Render for GridView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pager.total != self.dataset_count() {
            self.pager = Pager::new(self.dataset_count());
        }
        self.note_frame();
        self.pace_frame(window);
        let theme = Theme::current(cx);
        self.viewport_rows = self.viewport_rows_for(window);
        let (first, last) = self.pager.range();
        let title = match &self.mode {
            Mode::Filtered { label, .. } => format!("{label} · "),
            Mode::All => String::new(),
        };
        let pages = self.pager.pages();
        let progress = if pages > 1 {
            format!("{title}{} rows · {pages} pages", thousands(self.dataset_count()))
        } else {
            format!("{title}{} rows", thousands(self.dataset_count()))
        };
        let range_label = if self.dataset_count() == 0 {
            "Rows 0".to_string()
        } else if self.continuous {
            format!("Rows 1-{}", thousands(self.dataset_count()))
        } else {
            format!("Rows {}-{}", thousands(first), thousands(last))
        };
        let strip = div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(div().child(progress))
            .child(button("first", "|‹", theme).on_click(cx.listener(|this, _, _, cx| {
                this.pager.first();
                cx.notify();
            })))
            .child(button("prev", "‹", theme).on_click(cx.listener(|this, _, _, cx| {
                this.pager.previous();
                cx.notify();
            })))
            .child(div().child(range_label))
            .child(button("next", "›", theme).on_click(cx.listener(|this, _, _, cx| {
                this.pager.next();
                cx.notify();
            })))
            .child(button("last", "›|", theme).on_click(cx.listener(|this, _, _, cx| {
                this.pager.last();
                cx.notify();
            })))
            .child(div().w(px(110.0)).flex_none().child(self.find.clone()))
            .child(button("find-prev", "↑", theme).on_click(cx.listener(|this, _, _, cx| this.run_find(false, cx))))
            .child(button("find-next", "↓", theme).on_click(cx.listener(|this, _, _, cx| this.run_find(true, cx))))
            .child(
                button("show", "Matches only", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.show_matching(cx);
                    cx.notify();
                })),
            )
            .child(button("compare", "Compare", theme).on_click(cx.listener(|this, _, _, cx| this.compare(false, cx))))
            .child(button("export", "Export…", theme))
            .child(
                button(
                    "continuous",
                    if self.continuous {
                        "Continuous ✓"
                    } else {
                        "Continuous"
                    },
                    theme,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.continuous = !this.continuous;
                    this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    cx.notify();
                })),
            )
            .child(
                div()
                    .ml_auto()
                    .whitespace_nowrap()
                    .text_color(rgb(theme.muted_foreground))
                    .child(self.status.clone()),
            );
        let filter_row = self.filter_visible.then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .pb_2()
                .child(div().child("WHERE"))
                .child(div().w(px(420.0)).child(self.filter.clone()))
                .child(button("apply", "Apply", theme).on_click(cx.listener(|this, _, _, cx| this.apply_filter(cx))))
                .child(
                    button("clear", "Clear all", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.clear_filter();
                        cx.notify();
                    })),
                )
        });
        let compare_bar = self.compare.as_ref().map(|diff| {
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
        });
        let header = row_div(false, false, theme)
            .h(px(HEADER_HEIGHT))
            .bg(rgb(theme.muted))
            .border_b_1()
            .border_color(rgb(theme.border))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(theme.muted_foreground))
            .child(fixed_cell(GUTTER_WIDTH, "#".into()))
            .children(
                self.source
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(index, column)| header_cell(self.cell_cursor, index, column.width, &column.name, theme)),
            );
        let list = uniform_list(
            "rows",
            self.visible_count(),
            cx.processor(|this, range: Range<usize>, _window, cx| {
                range
                    .map(|index| this.row_at_position(index, cx))
                    .collect::<Vec<Stateful<Div>>>()
            }),
        )
        .track_scroll(&self.scroll)
        .flex_1();
        let palette = self.palette_open.then(|| self.palette_overlay(cx));
        self.sync_table(cx);
        let grid = div()
            .flex_1()
            .capture_key_down(
                cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| this.capture_key_down(event, window, cx)),
            )
            .flex()
            .flex_col()
            .min_h_0()
            .child(strip)
            .children(filter_row)
            .children(compare_bar)
            .children(self.compare.is_some().then(|| header))
            .children(self.compare.is_some().then(|| list))
            .children(self.compare.is_none().then(|| self.data_table()))
            .children(self.detail_panel(theme));
        let content = match self.tabs.current().clone() {
            Tab::Home => self.home_page(theme, cx),
            Tab::Query => self.query_page(theme, grid, cx),
            Tab::Table(name) if name == self.catalog.grid_table => grid,
            Tab::Table(name) => self.placeholder_page(&name, theme),
        };
        let sidebar = self.sidebar_open.then(|| self.sidebar(theme, cx));
        let toolbar = self.toolbar(theme, cx);
        let tab_bar = self.tab_bar(theme, cx);
        let tab_row = tab_bar;
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_size(px(13.0))
            .when(!self.font.is_empty(), |root| root.font_family(self.font.clone()))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                if this.resizing_sidebar {
                    this.sidebar_width = f32::from(event.position.x).clamp(180.0, 480.0);
                    cx.notify();
                }
                if this.resizing_split {
                    let height = f32::from(window.viewport_size().height) - 72.0 - 24.0;
                    this.result_ratio = result_ratio_from_drag(f32::from(event.position.y), 72.0, height);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseUpEvent, _, cx| {
                    let was_resizing =
                        std::mem::take(&mut this.resizing_sidebar) | std::mem::take(&mut this.resizing_split);
                    if was_resizing {
                        cx.notify();
                    }
                }),
            )
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .children(sidebar)
                    .child(div().flex_1().flex().flex_col().min_w_0().child(tab_row).child(content)),
            )
            .child(self.status_bar(theme))
            .children(palette)
    }
}

fn header_cell(cursor: Option<Cursor>, column: usize, width: f32, text: &str, theme: Theme) -> Div {
    let cell = cell_div(width, text.to_string());
    let cell = if column == 0 { cell.justify_end() } else { cell };
    match super::grid_edit::cursor_column_background(cursor, column, theme) {
        Some(background) => cell.bg(rgb(background)),
        None => cell,
    }
}

impl GridView {
    fn palette_overlay(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let actions = self.palette_actions();
        let shown = palette_matches(self.palette_input.read(cx).content(), &actions);
        let selected = self.palette_index.min(shown.len().saturating_sub(1));
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
        div().absolute().top(px(90.0)).w_full().flex().justify_center().child(
            div()
                .w(px(520.0))
                .p_2()
                .gap_1()
                .flex()
                .flex_col()
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.background))
                .shadow_lg()
                .child(self.palette_input.clone())
                .children(list)
                .children(empty),
        )
    }

    pub(super) fn row_at_position(&self, position: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::current(cx);
        if let Some(diff) = &self.compare {
            let Some(row) = diff.rows.get(position) else {
                return row_div(false, false, theme).id(position);
            };
            return row_div(position % 2 == 1, false, theme)
                .id(position)
                .child(fixed_cell(70.0, row.change.to_string()))
                .children(
                    self.source
                        .columns()
                        .iter()
                        .zip(&row.cells)
                        .map(|(column, cell)| display_cell(column.width, cell, theme)),
                );
        }
        let dataset_index = self.base() + position as u64;
        let source = self.source_row(dataset_index);
        let cells = self.source.row(source);
        let selected = self
            .selection
            .is_some_and(|selection| selection.contains(dataset_index));
        let highlighted = selected || super::grid_edit::cursor_row(self.cell_cursor, position);
        row_div(position % 2 == 1, highlighted, theme)
            .id(position)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.click_row(dataset_index, event.modifiers.shift);
                    cx.notify();
                }),
            )
            .child(
                fixed_cell(GUTTER_WIDTH, (dataset_index + 1).to_string())
                    .justify_end()
                    .text_color(rgb(theme.muted_foreground)),
            )
            .children(
                cells
                    .iter()
                    .enumerate()
                    .map(|(column, cell)| self.data_cell(source, column, position, cell, cx)),
            )
    }
}
