use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use gpui::{App, Context, IntoElement, WeakEntity, Window, div, prelude::*, px};
use gpui_kit::component::Sizable;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableState};

use crate::GridView;
use crate::column_projection::ColumnProjection;
use crate::grid_edit::Edits;
use crate::ui_theme::ThemeExt;
use bookie_gpui::model::{Cell, Diff};
use bookie_gpui::preview::cell_preview;
use bookie_gpui::source::Source;
use bookie_gpui::theme::Theme;

#[derive(Default)]
pub struct RowMap {
    base: u64,
    count: usize,
    kept: Option<Arc<Vec<u32>>>,
    compare: Option<Arc<Diff>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PinScope {
    connection: String,
    database: String,
    schema: Option<String>,
    table: String,
}

impl PartialEq for RowMap {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.count == other.count
            && same_arc(&self.kept, &other.kept)
            && same_arc(&self.compare, &other.compare)
    }
}

fn same_arc<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

impl RowMap {
    fn source_row(&self, row: usize) -> Option<u64> {
        let dataset = self.base + row as u64;
        match &self.kept {
            None => Some(dataset),
            Some(kept) => kept.get(dataset as usize).map(|index| u64::from(*index)),
        }
    }
}

#[derive(Default)]
pub struct GridDelegate {
    owner: Option<WeakEntity<GridView>>,
    sortable: bool,
    sort: Option<bookie_app_services::read_session::TableSort>,
    columns: Vec<(String, f32)>,
    projection: ColumnProjection,
    hidden_columns: HashSet<usize>,
    content_widths: Vec<f32>,
    available_width: f32,
    numeric: Vec<bool>,
    kinds: Vec<(String, bool)>,
    source: Option<Source>,
    map: RowMap,
    cached: Option<(usize, Vec<(String, bool)>)>,
    edits: Edits,
    editor: Option<(usize, usize, gpui::AnyView)>,
    pin_scope: Option<PinScope>,
    pinned_columns: usize,
    pin_counts: HashMap<PinScope, usize>,
    pub binary_modes: HashMap<usize, bookie_gpui::binary_mode::BinaryMode>,
}

fn header_menu(
    col_ix: usize,
    source_col: usize,
    column_name: String,
    owner: Option<WeakEntity<GridView>>,
    sortable: bool,
    has_hidden: bool,
    pin_action: Option<(gpui::Entity<TableState<GridDelegate>>, PinScope, bool)>,
    binary_action: Option<gpui::Entity<TableState<GridDelegate>>>,
    icon: impl IntoElement,
) -> impl IntoElement {
    use gpui_component::button::{Button, ButtonVariants};
    use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};

    Button::new(format!("column-menu-{col_ix}"))
        .ghost()
        .compact()
        .tooltip(format!("Actions for {column_name}"))
        .accessibility_label(format!("Actions for {column_name}"))
        .child(icon)
        .dropdown_menu(move |menu, _, _| {
            let copy_name = column_name.clone();
            let mut menu =
                menu.item(PopupMenuItem::new("Copy column name").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_name.clone()))
                }));
            if let Some(owner) = owner.clone() {
                let column = column_name.clone();
                let hide_owner = owner.clone();
                menu = menu.item(PopupMenuItem::new("Hide column").on_click(move |_, window, cx| {
                    let _ = hide_owner.update(cx, |view, cx| view.hide_column(source_col, &column, window, cx));
                }));
                if has_hidden {
                    menu = menu.item(
                        PopupMenuItem::new("Show all hidden columns").on_click(move |_, window, cx| {
                            let _ = owner.update(cx, |view, cx| view.show_all_hidden_columns(window, cx));
                        }),
                    );
                }
            }
            if let Some((table, scope, pinned)) = pin_action
                .as_ref()
                .map(|(table, scope, pinned)| (table.clone(), scope.clone(), *pinned))
            {
                let label = if pinned {
                    format!("Unpin from {column_name}")
                } else {
                    format!("Pin through {column_name}")
                };
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = table.update(cx, |table, cx| {
                        let delegate = table.delegate_mut();
                        delegate.pinned_columns =
                            next_pinned_count(delegate.pinned_columns, col_ix, delegate.columns.len());
                        delegate.pin_counts.insert(scope.clone(), delegate.pinned_columns);
                        table.refresh(cx);
                    });
                }));
            }
            if let Some(table) = binary_action.as_ref() {
                for mode in bookie_gpui::binary_mode::BinaryMode::ALL {
                    let label = format!("Display as {}", mode.label());
                    let mode_table = table.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let _ = mode_table.update(cx, |table, cx| {
                            table.delegate_mut().binary_modes.insert(source_col, mode);
                            table.delegate_mut().cached = None;
                            table.refresh(cx);
                        });
                    }));
                }
            }
            let Some(owner) = owner.clone().filter(|_| sortable) else {
                return menu;
            };
            for (label, ascending) in [("Sort ascending", true), ("Sort descending", false)] {
                let owner = owner.clone();
                let column = column_name.clone();
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.sort_current_column(source_col, &column, Some(ascending), window, cx);
                    });
                }));
            }
            menu
        })
}

impl TableDelegate for GridDelegate {
    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        if !self.sortable {
            return;
        }
        let Some(source_col) = self.projection.source_index(col_ix) else {
            return;
        };
        let Some((column, _)) = self.columns.get(source_col) else {
            return;
        };
        let column = column.clone();
        let ascending = match sort {
            ColumnSort::Ascending => Some(true),
            ColumnSort::Descending => Some(false),
            ColumnSort::Default => None,
        };
        let Some(owner) = self.owner.clone() else {
            return;
        };
        Self::schedule_sort(owner, source_col, column, ascending, window);
    }

    fn visible_rows_changed(&mut self, visible: Range<usize>, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let Some(source) = &self.source else { return };
        if self.map.compare.is_some()
            || self.content_widths.len() != self.columns.len()
            || !visible.clone().take(64).any(|row| {
                self.map
                    .source_row(row)
                    .is_some_and(|index| source.row_ref(index).is_some())
            })
        {
            return;
        }
        let measured = measure_visible_columns(
            source,
            &self.map,
            &self.columns,
            &self.edits,
            visible.start,
            visible.len(),
            |text| measure_text(text, window),
        );
        for (current, measured) in self.content_widths.iter_mut().zip(measured) {
            *current = current.max(measured);
        }
        let mut columns = self.columns.clone();
        for ((_, width), measured) in columns.iter_mut().zip(&self.content_widths) {
            *width = *measured;
        }
        fill_text_columns(&mut columns, &self.numeric, self.available_width);
        if columns == self.columns {
            return;
        }
        self.columns = columns;
        let table = cx.entity();
        window.on_next_frame(move |_, cx| {
            let _ = table.update(cx, |table, cx| table.refresh(cx));
        });
    }

    fn render_empty(&mut self, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .justify_center()
            .pt_6()
            .text_color(gpui::rgb(Theme::current(cx).muted_foreground))
            .child(empty_message(&self.map))
    }

    fn columns_count(&self, _cx: &App) -> usize {
        self.projection.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.map.count
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let source_col = self.projection.source_index(col_ix).unwrap_or(col_ix);
        let (name, width) = &self.columns[source_col];
        let mut column = Column::new(name.clone(), name.clone())
            .width(px(width.max(MIN_GRID_COLUMN_WIDTH)))
            .min_width(px(MIN_GRID_COLUMN_WIDTH))
            .resizable(true);
        if self.sortable {
            column = match self.sort.as_ref().filter(|sort| sort.column == *name) {
                Some(sort) if sort.ascending => column.ascending(),
                Some(_) => column.descending(),
                None => column.sortable(),
            };
        }
        if col_ix < self.pinned_columns {
            column.fixed_left()
        } else {
            column
        }
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let theme = Theme::current(cx);
        let source_col = self.projection.source_index(col_ix).unwrap_or(col_ix);
        let name = self
            .columns
            .get(source_col)
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        let kind = self
            .kinds
            .get(source_col)
            .map(|(kind, key)| header_kind(kind, *key))
            .unwrap_or_default();
        let column_width = self.columns.get(source_col).map_or(0.0, |(_, width)| *width);
        let show_pin_button = pin_button_visible(column_width);
        let is_binary = self
            .kinds
            .get(source_col)
            .is_some_and(|(k, _)| bookie_gpui::binary_mode::is_binary_type(k));
        let menu = header_menu(
            col_ix,
            source_col,
            name.clone(),
            self.owner.clone(),
            self.sortable,
            !self.hidden_columns.is_empty(),
            self.pin_scope
                .clone()
                .filter(|_| !show_pin_button)
                .map(|scope| (cx.entity(), scope, col_ix < self.pinned_columns)),
            is_binary.then(|| cx.entity()),
            crate::assets::icon("ellipsis", theme.muted_foreground),
        );
        let pin = self.pin_scope.clone().filter(|_| show_pin_button).map(|scope| {
            use gpui_component::button::{Button, ButtonVariants};

            let pinned = col_ix < self.pinned_columns;
            let action = if pinned { "Unpin from" } else { "Pin through" };
            let label = format!("{action} {name}");
            let color = if pinned {
                theme.foreground
            } else {
                theme.muted_foreground
            };
            Button::new(format!("pin-column-{col_ix}"))
                .ghost()
                .compact()
                .tooltip(label.clone())
                .accessibility_label(label.clone())
                .on_click(cx.listener(move |table, _, _, cx| {
                    let delegate = table.delegate_mut();
                    delegate.pinned_columns =
                        next_pinned_count(delegate.pinned_columns, col_ix, delegate.columns.len());
                    delegate.pin_counts.insert(scope.clone(), delegate.pinned_columns);
                    table.refresh(cx);
                }))
                .child(crate::assets::icon("pin", color))
        });
        div()
            .size_full()
            .min_w_0()
            .flex()
            .justify_center()
            .overflow_hidden()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .child(
                        div()
                            .id(("column-header-name", col_ix))
                            .debug_selector(move || format!("column-header-name-{col_ix}"))
                            .tooltip({
                                let full_name = name.clone();
                                move |window, cx| {
                                    gpui_component::tooltip::Tooltip::new(full_name.clone()).build(window, cx)
                                }
                            })
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(name),
                    )
                    .child(menu)
                    .children(pin),
            )
            .when(!kind.is_empty(), |el| {
                el.child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_xs()
                        .text_color(gpui::rgb(theme.muted_foreground))
                        .child(kind),
                )
            })
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let source_col = self.projection.source_index(col_ix).unwrap_or(col_ix);
        let is_numeric = self.numeric.get(source_col).copied().unwrap_or(false);
        let edited_tint = self
            .is_edited(row_ix, source_col)
            .then(|| gpui::rgb(Theme::current(cx).edited));
        match &self.editor {
            Some((row, column, view)) if *row == row_ix && *column == source_col => {
                div().w_full().flex().when(is_numeric, |el| el.justify_end()).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .when(is_numeric, |el| el.text_right())
                        .child(view.clone()),
                )
            }
            _ => {
                let (text, right) = self.label_of(row_ix, source_col);
                div()
                    .w_full()
                    .flex()
                    .when_some(edited_tint, |el, tint| el.bg(tint))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .when(right, |cell| cell.text_right())
                            .child(text),
                    )
            }
        }
    }
}

impl GridDelegate {
    pub(crate) fn new(owner: WeakEntity<GridView>) -> Self {
        Self {
            owner: Some(owner),
            ..Default::default()
        }
    }

    pub(crate) fn source_column(&self, visible: usize) -> Option<usize> {
        self.projection.source_index(visible)
    }

    pub(crate) fn clone_projection(&self) -> ColumnProjection {
        self.projection.clone()
    }

    pub(crate) fn set_hidden_columns(&mut self, column_count: usize, hidden: HashSet<usize>) {
        self.projection = ColumnProjection::new(column_count, &hidden);
        self.hidden_columns = hidden;
    }

    fn schedule_sort(
        owner: WeakEntity<GridView>,
        column_index: usize,
        column: String,
        ascending: Option<bool>,
        window: &mut Window,
    ) {
        window.on_next_frame(move |_, app| {
            let _ = owner.update_in(app, |view, window, cx| {
                view.sort_current_column(column_index, &column, ascending, window, cx);
            });
        });
    }

    fn is_edited(&self, row_ix: usize, col_ix: usize) -> bool {
        self.map.compare.is_none()
            && self
                .map
                .source_row(row_ix)
                .is_some_and(|index| self.edits.contains_key(&(index, col_ix)))
    }

    fn is_changed(
        &self,
        source: &Source,
        columns: &[(String, f32)],
        numeric: &[bool],
        kinds: &[(String, bool)],
        map: &RowMap,
        edits: &Edits,
        editor: &Option<(usize, usize, gpui::AnyView)>,
        sortable: bool,
        sort: Option<&bookie_app_services::read_session::TableSort>,
    ) -> bool {
        let same_source = self.source.as_ref().is_some_and(|current| Arc::ptr_eq(current, source));
        !same_source
            || self.columns != columns
            || self.numeric != numeric
            || self.kinds != kinds
            || self.sortable != sortable
            || self.sort.as_ref() != sort
            || self.map != *map
            || self.edits != *edits
            || self.editor.as_ref().map(|(row, column, _)| (*row, *column))
                != editor.as_ref().map(|(row, column, _)| (*row, *column))
    }

    pub(crate) fn label_of(&mut self, row_ix: usize, col_ix: usize) -> (String, bool) {
        if self.map.compare.is_none()
            && let Some(index) = self.map.source_row(row_ix)
            && let Some(cells) = self.source.as_ref().and_then(|source| source.row_ref(index))
        {
            return cells.get(col_ix).map_or_else(Default::default, |cell| {
                let cell = self.edits.get(&(index, col_ix)).unwrap_or(cell);
                let right = crate::right_aligned(self.numeric.get(col_ix).copied().unwrap_or(false), cell);
                let preview = match (cell, self.binary_modes.get(&col_ix)) {
                    (Cell::Bytes(bytes), Some(mode)) => mode.format(bytes),
                    _ => cell_preview(cell),
                };
                (preview, right)
            });
        }
        if self.cached.as_ref().is_none_or(|(row, _)| *row != row_ix) {
            let numeric = &self.numeric;
            let binary_modes = &self.binary_modes;
            let labels = match (&self.map.compare, &self.source) {
                (Some(diff), _) => compare_labels(diff, row_ix, numeric),
                (None, Some(source)) => match self.map.source_row(row_ix) {
                    Some(index) => source_labels(source, index, numeric, &self.edits, binary_modes),
                    None => Vec::new(),
                },
                (None, None) => Vec::new(),
            };
            self.cached = Some((row_ix, labels));
        }
        self.cached
            .as_ref()
            .and_then(|(_, labels)| labels.get(col_ix))
            .cloned()
            .unwrap_or_default()
    }
}

impl GridView {
    fn pin_scope(&self) -> Option<PinScope> {
        let bookie_gpui::shell::Tab::Table(table) = self.tabs.current() else {
            return None;
        };
        Some(PinScope {
            connection: self.live.as_ref().map_or_else(
                || self.catalog.connection.clone(),
                |session| {
                    session.saved.as_ref().map_or_else(
                        || format!("{}:{}", session.driver_id, session.label),
                        |(id, _)| id.to_string(),
                    )
                },
            ),
            database: self.catalog.database.clone(),
            schema: table.schema.clone(),
            table: table.name.clone(),
        })
    }

    fn row_map(&self) -> RowMap {
        if let Some(diff) = &self.compare {
            return RowMap {
                base: 0,
                count: diff.rows.len(),
                kept: None,
                compare: Some(diff.clone()),
            };
        }
        RowMap {
            base: self.base(),
            count: self.visible_count(),
            kept: match &self.mode {
                crate::Mode::All => self.local_order.clone(),
                crate::Mode::Filtered { kept, .. } => self.local_order.clone().or_else(|| Some(kept.clone())),
            },
            compare: None,
        }
    }

    fn grid_columns(&self) -> (Vec<(String, f32)>, Vec<bool>, Vec<(String, bool)>) {
        let mut columns: Vec<_> = self
            .source
            .columns()
            .iter()
            .map(|column| (column.name.clone(), column.width))
            .collect();
        let mut numeric: Vec<_> = self.source.columns().iter().map(|column| column.numeric).collect();
        let mut kinds: Vec<_> = self
            .source
            .columns()
            .iter()
            .map(|column| (column.data_type.clone(), column.primary_key))
            .collect();
        if self.compare.is_some() {
            columns.insert(0, ("Change".into(), COMPARE_CHANGE_WIDTH));
            numeric.insert(0, false);
            kinds.insert(0, (String::new(), false));
        }
        (columns, numeric, kinds)
    }

    fn column_projection(
        &self,
        columns: &[(String, f32)],
        delegate: &GridDelegate,
        pin_scope: &Option<PinScope>,
    ) -> (HashSet<usize>, ColumnProjection) {
        let persisted = self
            .live
            .as_ref()
            .filter(|_| !self.grid_is_query && self.compare.is_none())
            .and_then(|session| session.saved.as_ref().map(|(id, _)| (*id, session)))
            .and_then(|(id, _)| {
                self.column_widths
                    .as_ref()
                    .map(|store| store.hidden_columns(id, &self.catalog.grid_table.name))
            });
        let scope_changed = delegate.pin_scope.as_ref() != pin_scope.as_ref()
            || delegate
                .source
                .as_ref()
                .is_none_or(|source| !Arc::ptr_eq(source, &self.source));
        let hidden: HashSet<usize> = persisted
            .map(|hidden_names| {
                let names = columns.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
                crate::column_projection::hidden_from_names(&names, &hidden_names)
            })
            .unwrap_or_else(|| {
                if scope_changed {
                    HashSet::new()
                } else {
                    delegate.hidden_columns.clone()
                }
            });
        let projection = ColumnProjection::new(columns.len(), &hidden);
        (hidden, projection)
    }

    fn sort_state(&self) -> (bool, Option<bookie_app_services::read_session::TableSort>) {
        let server_sortable = !self.grid_is_query
            && self.compare.is_none()
            && self
                .live
                .as_ref()
                .is_some_and(|session| session.driver_id == "postgres");
        let local_sortable = (self.grid_is_query
            || self
                .live
                .as_ref()
                .is_some_and(|session| session.driver_id != "postgres"))
            && self.compare.is_none()
            && self.dataset_count() <= crate::local_sort::MAX_LOCAL_SORT_ROWS
            && !self.source.is_partial()
            && (self.dataset_count() == 0 || self.source.row_ref(0).is_some());
        let sortable = server_sortable || local_sortable;
        let sort = if local_sortable {
            self.local_sort.clone()
        } else {
            server_sortable
                .then(|| self.table_sort(&self.catalog.grid_table))
                .flatten()
                .cloned()
        };
        (sortable, sort)
    }

    pub(crate) fn sync_table(&self, available_width: f32, window: &mut Window, cx: &mut Context<Self>) {
        let (mut columns, numeric, kinds) = self.grid_columns();
        let map = self.row_map();
        let pin_scope = self.pin_scope();
        let table = self.table.read(cx);
        let delegate = table.delegate();
        let (hidden_columns, projection) = self.column_projection(&columns, delegate, &pin_scope);
        let widths_match = delegate
            .source
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && delegate.map == map
            && delegate.edits == self.cell_edit.edits
            && delegate.content_widths.len() == columns.len();
        let content_widths = if widths_match {
            delegate.content_widths.clone()
        } else {
            let start = table.vertical_scroll_handle.0.borrow().base_handle.top_item();
            measure_visible_columns(
                &self.source,
                &map,
                &columns,
                &self.cell_edit.edits,
                start,
                self.viewport_rows,
                |text| measure_text(text, window),
            )
        };
        for ((_, width), measured) in columns.iter_mut().zip(&content_widths) {
            *width = *measured;
        }
        fill_text_columns(&mut columns, &numeric, available_width);
        let (sortable, sort) = self.sort_state();
        let editor = self.cell_edit.active.as_ref().and_then(|editing| {
            let dataset = self.dataset_index_of(editing.row)?.checked_sub(map.base)?;
            (dataset < map.count as u64).then(|| {
                (
                    dataset as usize,
                    editing.column,
                    gpui::AnyView::from(self.cell_edit.editor.clone()),
                )
            })
        });
        self.table.update(cx, |table, cx| {
            let cell = table_cell(self.cell_cursor)
                .map(|(row, source_col)| (row, projection.visible_index(source_col).unwrap_or(0)))
                .or((map.count > 0).then_some((0, 0)));
            if table.selected_cell() != cell {
                match cell {
                    Some((row, column)) => table.set_selected_cell(row, column, cx),
                    None => table.clear_selection(cx),
                }
            }
            let delegate = table.delegate_mut();
            delegate.cached = None;
            delegate.content_widths = content_widths;
            delegate.available_width = available_width;
            let pinned_columns = pin_scope
                .as_ref()
                .and_then(|scope| delegate.pin_counts.get(scope))
                .copied()
                .unwrap_or(0)
                .min(columns.len());
            if !delegate.is_changed(
                &self.source,
                &columns,
                &numeric,
                &kinds,
                &map,
                &self.cell_edit.edits,
                &editor,
                sortable,
                sort.as_ref(),
            ) && delegate.projection == projection
                && delegate.pin_scope == pin_scope
                && delegate.pinned_columns == pinned_columns
            {
                return;
            }
            delegate.columns = columns;
            delegate.hidden_columns = hidden_columns;
            delegate.projection = projection;
            delegate.sortable = sortable;
            delegate.sort = sort;
            delegate.numeric = numeric;
            delegate.kinds = kinds;
            delegate.source = Some(self.source.clone());
            delegate.map = map;
            delegate.edits = self.cell_edit.edits.clone();
            delegate.editor = editor;
            delegate.pin_scope = pin_scope;
            delegate.pinned_columns = pinned_columns;
            table.refresh(cx);
        });
    }

    pub(crate) fn data_table(&self, cx: &App) -> impl IntoElement {
        let (vertical_scroll, horizontal_scroll) = {
            let table = self.table.read(cx);
            (
                table.vertical_scroll_handle.clone(),
                table.horizontal_scroll_handle.clone(),
            )
        };
        let gutter = Scrollbar::width();
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("grid-table-viewport")
                            .debug_selector(|| "grid-table-viewport".into())
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                DataTable::new(&self.table)
                                    .scrollbar_visible(false, false)
                                    .stripe(true)
                                    .xsmall()
                                    .header_row_height(px(bookie_gpui::theme::size::GRID_HEADER)),
                            ),
                    )
                    .child(
                        div()
                            .id("grid-horizontal-scrollbar-gutter")
                            .debug_selector(|| "grid-horizontal-scrollbar-gutter".into())
                            .h(gutter)
                            .flex_none()
                            .child(
                                Scrollbar::horizontal(&horizontal_scroll)
                                    .id("grid-horizontal-scrollbar")
                                    .viewport_from_layout(),
                            ),
                    ),
            )
            .child(
                div()
                    .w(gutter)
                    .flex_none()
                    .flex()
                    .flex_col()
                    .child(div().h(px(bookie_gpui::theme::size::GRID_HEADER)).flex_none())
                    .child(
                        div()
                            .id("grid-vertical-scrollbar-gutter")
                            .debug_selector(|| "grid-vertical-scrollbar-gutter".into())
                            .flex_1()
                            .min_h_0()
                            .child(
                                Scrollbar::vertical(&vertical_scroll)
                                    .id("grid-vertical-scrollbar")
                                    .viewport_from_layout(),
                            ),
                    ),
            )
    }

    fn hide_column(&mut self, source_col: usize, name: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let columns = self.table.read(cx).delegate().columns.clone();
        if columns.get(source_col).is_none_or(|(column, _)| column != name) {
            return;
        }
        let mut hidden = self.table.read(cx).delegate().hidden_columns.clone();
        if hidden.contains(&source_col) {
            return;
        }
        let projection = ColumnProjection::new(columns.len(), &hidden);
        if projection.len() <= 1 {
            self.status = "At least one column must remain visible.".into();
            return;
        }
        hidden.insert(source_col);
        let unique_name = columns.iter().filter(|(column, _)| column == name).count() == 1;
        if !self.grid_is_query
            && self.compare.is_none()
            && let Some((id, _)) = self.live.as_ref().and_then(|session| session.saved.as_ref())
            && let Some(store) = &self.column_widths
            && unique_name
            && let Err(error) = store.set_hidden(*id, &self.catalog.grid_table.name, name, true)
        {
            tracing::warn!(%error, "could not persist the hidden GPUI grid column");
        }
        let projection = ColumnProjection::new(columns.len(), &hidden);
        self.table.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            delegate.hidden_columns = hidden;
            delegate.projection = projection;
            table.refresh(cx);
        });
        let compare_offset = usize::from(self.compare.is_some());
        if self
            .cell_cursor
            .is_some_and(|cursor| cursor.column + compare_offset == source_col)
        {
            let next_col = self
                .table
                .read(cx)
                .delegate()
                .projection
                .source_index(0)
                .unwrap_or(compare_offset)
                .saturating_sub(compare_offset);
            if let Some(cursor) = &mut self.cell_cursor {
                cursor.column = next_col;
            }
        }
        self.status = format!("Hidden column {name}");
        cx.notify();
    }

    fn show_all_hidden_columns(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (hidden, columns) = {
            let table = self.table.read(cx);
            let delegate = table.delegate();
            (delegate.hidden_columns.clone(), delegate.columns.clone())
        };
        if hidden.is_empty() {
            return;
        }
        if !self.grid_is_query
            && self.compare.is_none()
            && let Some((id, _)) = self.live.as_ref().and_then(|session| session.saved.as_ref())
            && let Some(store) = &self.column_widths
        {
            for source_col in &hidden {
                let Some((name, _)) = columns.get(*source_col) else {
                    continue;
                };
                if columns.iter().filter(|(column, _)| column == name).count() != 1 {
                    continue;
                }
                if let Err(error) = store.set_hidden(*id, &self.catalog.grid_table.name, name, false) {
                    tracing::warn!(%error, column = %name, "could not restore a hidden GPUI grid column preference");
                }
            }
        }
        let projection = ColumnProjection::new(columns.len(), &HashSet::new());
        self.table.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            delegate.hidden_columns.clear();
            delegate.projection = projection;
            table.refresh(cx);
        });
        cx.notify();
    }
}

const COMPARE_CHANGE_WIDTH: f32 = 70.0;
const MIN_GRID_COLUMN_WIDTH: f32 = 72.0;
const PIN_BUTTON_MIN_COLUMN_WIDTH: f32 = 96.0;

fn pin_button_visible(column_width: f32) -> bool {
    column_width >= PIN_BUTTON_MIN_COLUMN_WIDTH
}

fn next_pinned_count(current: usize, clicked: usize, column_count: usize) -> usize {
    if clicked < current {
        clicked
    } else {
        clicked.saturating_add(1).min(column_count)
    }
}

#[cfg(test)]
mod pin_tests {
    use super::{next_pinned_count, pin_button_visible};

    #[test]
    fn pin_actions_keep_a_contiguous_leading_prefix() {
        assert_eq!(next_pinned_count(0, 2, 60), 3);
        assert_eq!(next_pinned_count(3, 1, 60), 1);
        assert_eq!(next_pinned_count(3, 0, 60), 0);
        assert_eq!(next_pinned_count(3, 99, 60), 60);
    }

    #[test]
    fn narrow_headers_move_pin_actions_into_the_menu() {
        assert!(!pin_button_visible(56.0));
        assert!(pin_button_visible(96.0));
    }
}

fn header_kind(data_type: &str, primary_key: bool) -> String {
    match (data_type.is_empty(), primary_key) {
        (true, false) => String::new(),
        (true, true) => "PK".into(),
        (false, false) => data_type.into(),
        (false, true) => format!("{data_type} · PK"),
    }
}

const MIN_CONTENT_WIDTH: f32 = 56.0;
const MAX_CONTENT_WIDTH: f32 = 360.0;
const MAX_MEASURE_CELLS: usize = 2048;

fn fill_text_columns(columns: &mut [(String, f32)], numeric: &[bool], available_width: f32) {
    if !available_width.is_finite() || available_width <= 0.0 {
        return;
    }
    let mut remaining = (available_width - columns.iter().map(|(_, width)| width).sum::<f32>()).max(0.0);
    while remaining > 0.5 {
        let flexible: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter_map(|(index, (_, width))| {
                (!numeric.get(index).copied().unwrap_or(false) && *width < MAX_CONTENT_WIDTH).then_some(index)
            })
            .collect();
        if flexible.is_empty() {
            break;
        }
        let share = remaining / flexible.len() as f32;
        let mut used = 0.0;
        for index in flexible {
            let add = share.min(MAX_CONTENT_WIDTH - columns[index].1);
            columns[index].1 += add;
            used += add;
        }
        if used <= 0.0 {
            break;
        }
        remaining -= used;
    }
}

fn measure_visible_columns(
    source: &Source,
    map: &RowMap,
    columns: &[(String, f32)],
    edits: &Edits,
    start: usize,
    viewport_rows: usize,
    mut measure: impl FnMut(&str) -> f32,
) -> Vec<f32> {
    let mut widths: Vec<f32> = columns
        .iter()
        .map(|(name, _)| measure(name).max(MIN_CONTENT_WIDTH).min(MAX_CONTENT_WIDTH))
        .collect();
    if columns.is_empty() || map.compare.is_some() {
        return widths;
    }
    let row_limit = viewport_rows.max(1).saturating_add(4).min(64);
    let cell_limit_rows = (MAX_MEASURE_CELLS / columns.len().max(1)).max(1);
    let end = start.saturating_add(row_limit.min(cell_limit_rows)).min(map.count);
    let mut borrowed_row = false;
    for row in start.min(end)..end {
        let Some(source_row) = map.source_row(row) else {
            continue;
        };
        let Some(cells) = source.row_ref(source_row) else {
            continue;
        };
        borrowed_row = true;
        for (column, cell) in cells.iter().take(widths.len()).enumerate() {
            let preview = cell_preview(edits.get(&(source_row, column)).unwrap_or(cell));
            widths[column] = widths[column].max(measure(&preview).min(MAX_CONTENT_WIDTH));
        }
    }
    if !borrowed_row {
        for (width, (_, fallback)) in widths.iter_mut().zip(columns) {
            *width = (*width).max((*fallback).min(MAX_CONTENT_WIDTH));
        }
    }
    widths
}

fn measure_text(text: &str, window: &Window) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let style = window.text_style();
    let normalized;
    let text = if text.contains(['\n', '\r', '\t']) {
        normalized = text.replace(['\n', '\r', '\t'], " ");
        normalized.as_str()
    } else {
        text
    };
    let run = gpui::TextRun {
        len: text.len(),
        font: style.font(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    f32::from(
        window
            .text_system()
            .shape_line(text.into(), style.font_size.to_pixels(window.rem_size()), &[run], None)
            .width,
    ) + 12.0
}

fn labels(cells: &[Cell], numeric: &[bool]) -> Vec<(String, bool)> {
    cells
        .iter()
        .enumerate()
        .map(|(column, cell)| {
            let right = crate::right_aligned(numeric.get(column).copied().unwrap_or(false), cell);
            (cell_preview(cell), right)
        })
        .collect()
}

fn source_labels(
    source: &Source,
    index: u64,
    numeric: &[bool],
    edits: &Edits,
    binary_modes: &HashMap<usize, bookie_gpui::binary_mode::BinaryMode>,
) -> Vec<(String, bool)> {
    match source.row_ref(index) {
        Some(cells) => labels_with_edits(cells, numeric, index, edits, binary_modes),
        None => labels_with_edits(&source.row(index), numeric, index, edits, binary_modes),
    }
}

fn labels_with_edits(
    cells: &[Cell],
    numeric: &[bool],
    row: u64,
    edits: &Edits,
    binary_modes: &HashMap<usize, bookie_gpui::binary_mode::BinaryMode>,
) -> Vec<(String, bool)> {
    cells
        .iter()
        .enumerate()
        .map(|(column, cell)| {
            let cell = edits.get(&(row, column)).unwrap_or(cell);
            let right = crate::right_aligned(numeric.get(column).copied().unwrap_or(false), cell);
            let preview = match (cell, binary_modes.get(&column)) {
                (Cell::Bytes(bytes), Some(mode)) => mode.format(bytes),
                _ => cell_preview(cell),
            };
            (preview, right)
        })
        .collect()
}

fn compare_labels(diff: &Diff, row: usize, numeric: &[bool]) -> Vec<(String, bool)> {
    let Some(row) = diff.rows.get(row) else {
        return Vec::new();
    };
    std::iter::once((row.change.to_string(), false))
        .chain(labels(&row.cells, numeric.get(1..).unwrap_or(&[])))
        .collect()
}

fn empty_message(map: &RowMap) -> &'static str {
    if map.kept.is_some() {
        "No rows match the filter"
    } else {
        "No rows"
    }
}

pub fn table_cell(cursor: Option<bookie_results::grid_cursor::Cursor>) -> Option<(usize, usize)> {
    cursor.map(|cursor| (cursor.row, cursor.column))
}

#[cfg(test)]
pub fn cursor_at(cell: Option<(usize, usize)>) -> Option<bookie_results::grid_cursor::Cursor> {
    cell.map(|(row, column)| bookie_results::grid_cursor::Cursor { row, column })
}

#[cfg(test)]
mod header_kind_tests {
    use super::header_kind;

    #[test]
    fn a_header_shows_the_type_and_marks_the_primary_key() {
        assert_eq!(header_kind("integer", true), "integer · PK");
        assert_eq!(header_kind("text", false), "text");
        assert_eq!(header_kind("", true), "PK");
        assert_eq!(header_kind("", false), "");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use bookie_gpui::model::Cell;
    use bookie_gpui::source::{ResultSource, Synthetic};
    use bookie_results::grid_cursor::Cursor;

    #[gpui::test]
    fn header_sort_indicator_matches_the_applied_sort(cx: &mut gpui::TestAppContext) {
        let (_, cx) = crate::ui_tests::open(cx);
        cx.update(|_, cx| {
            let mut delegate = GridDelegate {
                sortable: true,
                columns: vec![("name".into(), 100.0), ("id".into(), 100.0)],
                sort: Some(bookie_app_services::read_session::TableSort {
                    column: "name".into(),
                    ascending: false,
                }),
                ..Default::default()
            };
            assert_eq!(delegate.column(0, cx).sort, Some(ColumnSort::Descending));
            assert_eq!(delegate.column(1, cx).sort, Some(ColumnSort::Default));
            delegate.sort = None;
            assert_eq!(delegate.column(0, cx).sort, Some(ColumnSort::Default));
        });
    }

    #[test]
    fn an_empty_grid_says_whether_a_filter_or_the_table_is_empty() {
        assert_eq!(empty_message(&RowMap::default()), "No rows");
        let filtered = RowMap {
            kept: Some(Arc::new(Vec::new())),
            ..RowMap::default()
        };
        assert_eq!(empty_message(&filtered), "No rows match the filter");
    }

    #[test]
    fn a_cell_reads_the_source_row_it_maps_to_and_keeps_that_row_cached() {
        let source: Source = Arc::new(Synthetic::new(10));
        let mut delegate = GridDelegate {
            source: Some(source),
            map: RowMap {
                base: 0,
                count: 2,
                kept: Some(Arc::new(vec![3, 7])),
                compare: None,
            },
            ..Default::default()
        };
        let expected = cell_preview(&Synthetic::new(10).row(7)[1]);
        assert_eq!(delegate.label_of(1, 1).0, expected);
        assert_eq!(delegate.cached.as_ref().map(|(row, _)| *row), Some(1));
        assert_eq!(delegate.label_of(1, 1).0, expected);
        assert_eq!(delegate.label_of(9, 0).0, "");
    }

    struct BorrowedRowSource {
        cells: Vec<Cell>,
        row_copies: AtomicUsize,
    }

    impl ResultSource for BorrowedRowSource {
        fn columns(&self) -> &[bookie_gpui::model::ColumnMeta] {
            &[]
        }

        fn total(&self) -> u64 {
            1
        }

        fn row(&self, _index: u64) -> Vec<Cell> {
            self.row_copies.fetch_add(1, Ordering::Relaxed);
            self.cells.clone()
        }

        fn row_ref(&self, index: u64) -> Option<&[Cell]> {
            (index == 0).then_some(&self.cells)
        }

        fn find(
            &self,
            _query: &str,
            _cap: usize,
            _cancel: &tokio_util::sync::CancellationToken,
        ) -> Result<bookie_gpui::model::FindResult, &'static str> {
            Err("not available")
        }

        fn filter(
            &self,
            _predicate: &bookie_gpui::model::Predicate,
            _cancel: &tokio_util::sync::CancellationToken,
        ) -> Result<Vec<u32>, &'static str> {
            Err("not available")
        }

        fn compare(
            &self,
            _keyed: bool,
            _cancel: &tokio_util::sync::CancellationToken,
        ) -> Result<bookie_gpui::model::Diff, &'static str> {
            Err("not available")
        }
    }

    #[test]
    fn live_grid_previews_borrow_large_cells_instead_of_cloning_the_row() {
        let source = Arc::new(BorrowedRowSource {
            cells: vec![Cell::Text("x".repeat(1024 * 1024))],
            row_copies: AtomicUsize::new(0),
        });
        let source_dyn: Source = source.clone();
        let mut delegate = GridDelegate {
            columns: vec![("value".into(), 100.0)],
            numeric: vec![false],
            source: Some(source_dyn),
            map: RowMap {
                count: 1,
                ..Default::default()
            },
            ..Default::default()
        };

        let preview = delegate.label_of(0, 0).0;

        assert!(preview.starts_with(&"x".repeat(512)));
        assert!(preview.len() < 600);
        assert_eq!(source.row_copies.load(Ordering::Relaxed), 0);
        assert!(matches!(&source.cells[0], Cell::Text(value) if value.len() == 1024 * 1024));
    }

    #[test]
    fn visible_values_size_columns_without_cloning_rows_or_using_type_guesses() {
        let source = Arc::new(BorrowedRowSource {
            cells: vec![Cell::Int(7), Cell::Text("2026-11-20T16:44:22.133000+01:00".into())],
            row_copies: AtomicUsize::new(0),
        });
        let source_dyn: Source = source.clone();
        let columns = vec![("id".into(), 120.0), ("created_at".into(), 272.0)];
        let widths = measure_visible_columns(
            &source_dyn,
            &RowMap {
                count: 1,
                ..Default::default()
            },
            &columns,
            &Edits::default(),
            0,
            20,
            |text| text.chars().count() as f32 * 8.5 + 12.0,
        );

        assert_eq!(widths[0], MIN_CONTENT_WIDTH);
        assert!(widths[1] > columns[1].1);
        assert_eq!(source.row_copies.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn borrowed_wide_rows_format_only_requested_cells_and_keep_pending_edits() {
        let source = Arc::new(BorrowedRowSource {
            cells: vec![Cell::Int(7); 512],
            row_copies: AtomicUsize::new(0),
        });
        let source_dyn: Source = source.clone();
        let mut delegate = GridDelegate {
            source: Some(source_dyn),
            numeric: vec![true; 512],
            map: RowMap {
                count: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        delegate.edits.insert((0, 511), Cell::Int(9));
        assert_eq!(delegate.label_of(0, 0), ("7".into(), true));
        assert_eq!(delegate.label_of(0, 511), ("9".into(), true));
        assert_eq!(delegate.label_of(0, 512), (String::new(), false));
        assert!(delegate.cached.is_none());
        assert_eq!(source.row_copies.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_map_covers_only_its_own_page() {
        let plain = RowMap {
            base: 5_000,
            count: 2,
            kept: None,
            compare: None,
        };
        assert_eq!(plain.source_row(1), Some(5_001));
        let filtered = RowMap {
            base: 0,
            count: 2,
            kept: Some(Arc::new(vec![9, 4])),
            compare: None,
        };
        assert_eq!(filtered.source_row(1), Some(4));
        assert_eq!(filtered.source_row(2), None);
    }

    #[test]
    fn a_cursor_survives_the_trip_through_the_table_cell() {
        let cursor = Some(Cursor { row: 3, column: 2 });
        assert_eq!(cursor_at(table_cell(cursor)), cursor);
        assert_eq!(cursor_at(table_cell(None)), None);
    }

    #[test]
    fn a_table_cell_survives_the_trip_through_the_cursor() {
        assert_eq!(table_cell(cursor_at(Some((0, 5)))), Some((0, 5)));
        assert_eq!(table_cell(cursor_at(None)), None);
    }

    #[test]
    fn a_compare_row_shows_its_change_then_its_cells() {
        use bookie_gpui::model::{Diff, DiffRow, DiffSummary};
        let diff = Diff {
            summary: DiffSummary {
                keyed: false,
                equal: 0,
                added: 0,
                removed: 0,
                changed: 1,
                unknown: 0,
            },
            rows: vec![DiffRow {
                change: "changed",
                cells: vec![Cell::Text("a".into()), Cell::Int(7)],
            }],
            capped: false,
        };
        let mut delegate = GridDelegate {
            map: RowMap {
                base: 0,
                count: 1,
                kept: None,
                compare: Some(Arc::new(diff)),
            },
            ..Default::default()
        };
        assert_eq!(delegate.label_of(0, 0).0, "changed");
        assert_eq!(delegate.label_of(0, 2).0, "7");
    }

    #[test]
    fn numbers_and_nulls_in_numeric_columns_are_right_aligned() {
        let flags: Vec<bool> = labels(
            &[Cell::Int(7), Cell::Null, Cell::Text("x".into())],
            &[true, true, false],
        )
        .into_iter()
        .map(|(_, right)| right)
        .collect();
        assert_eq!(flags, vec![true, true, false]);
    }

    #[test]
    fn spare_width_is_shared_by_text_columns_without_widening_numbers() {
        let mut columns = vec![
            ("id".into(), 56.0),
            ("created_at".into(), 200.0),
            ("name".into(), 100.0),
        ];
        fill_text_columns(&mut columns, &[true, false, false], 800.0);
        assert_eq!(columns[0].1, 56.0);
        assert_eq!(columns[1].1, MAX_CONTENT_WIDTH);
        assert_eq!(columns[2].1, MAX_CONTENT_WIDTH);

        fill_text_columns(&mut columns, &[true, false, false], 250.0);
        assert_eq!(columns[0].1, 56.0);
        assert_eq!(columns[1].1, MAX_CONTENT_WIDTH);
        assert_eq!(columns[2].1, MAX_CONTENT_WIDTH);
    }

    #[test]
    fn an_empty_result_has_no_column_to_expand() {
        fill_text_columns(&mut [], &[], 520.0);
    }
}
