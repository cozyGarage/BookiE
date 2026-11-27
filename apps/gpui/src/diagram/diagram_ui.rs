use bookie_app_services::er_diagram::layout::Point;
use bookie_app_services::er_diagram::placement::{self, CardPlacement};
use bookie_app_services::er_diagram::scene::Rect;
use bookie_app_services::er_diagram::zoom;
use bookie_app_services::er_diagram::{DiagramTable, SchemaDiagram};
use bookie_core::TableInfo;
use gpui::{MouseDownEvent, PathBuilder, canvas, point};
use std::collections::BTreeSet;

use crate::diagram_scene::{Scene, build_scene};
use crate::surfaces::{ButtonKind, ListState, compact_button, compact_icon_button, list_state};
use crate::*;

const DIAGRAM_EDGE_STROKE_WIDTH: f32 = 1.5;

pub(crate) struct DiagramView {
    pub diagram: SchemaDiagram,
    pub scene: Scene,
    pub zoom: f64,
    pub needs_fit: bool,
    pub total: usize,
    pub incomplete: bool,
    pub selected: Option<usize>,
    pub neighbours_only: bool,
    placements: Vec<CardPlacement>,
    gesture: Option<CardGesture>,
    saving_layout: bool,
}

struct CardGesture {
    index: usize,
    pointer: gpui::Point<gpui::Pixels>,
    origin: Point,
    width: f64,
    resize: bool,
}

impl DiagramView {
    fn place_card(&mut self, index: usize, at: Point, width: f64) {
        let Some(table) = self.diagram.tables.get(index) else {
            return;
        };
        let placement = CardPlacement {
            schema: table.schema.clone(),
            table: table.name.clone(),
            x: at.x,
            y: at.y,
            width,
        };
        if !placement.valid() {
            return;
        }
        self.scene.place_card(&self.diagram, index, at, width);
        if let Some(previous) = self
            .placements
            .iter_mut()
            .find(|previous| previous.schema == placement.schema && previous.table == placement.table)
        {
            *previous = placement;
        } else {
            self.placements.push(placement);
        }
    }
}

fn px_at(value: f64) -> gpui::Pixels {
    px(value as f32)
}

impl GridView {
    pub(crate) fn diagram_actions(&self) -> Vec<Action> {
        let enabled = self.live.is_some();
        vec![Action {
            id: "show-schema-diagram",
            label: "Show the schema diagram".to_string(),
            enabled,
        }]
    }

    pub(crate) fn open_schema_diagram(&mut self, cx: &mut Context<Self>) {
        self.load_schema_diagram(self.search.diagram.scope.clone(), cx);
    }

    pub(crate) fn reset_diagram(&mut self) {
        let state = &mut self.search.diagram;
        state.request.invalidate();
        state.view = None;
        state.scope = None;
        state.loading = false;
        state.failure = None;
        state.find = None;
        state.ddl = None;
        state.relationship = None;
        state.scroll = Default::default();
    }

    pub(crate) fn load_schema_diagram(&mut self, scope: Option<String>, cx: &mut Context<Self>) {
        let Some(session) = self.live.clone() else {
            self.status = "Connect to a database to draw its schema".into();
            return;
        };
        self.tabs.open_diagram();
        self.sync_grid_owner();
        let state = &mut self.search.diagram;
        if state.scope != scope {
            state.view = None;
        }
        state.relationship = None;
        state.ddl = None;
        state.scope.clone_from(&scope);
        state.loading = true;
        state.failure = None;
        state.request.invalidate();
        state.request.cancel = CancellationToken::new();
        let cancel = state.request.cancel.clone();
        let arrangement = session.diagram_layout_path(scope.as_deref());
        let pending = state.view.as_ref().map(|view| view.placements.clone());
        let selection = state.view.as_ref().and_then(|view| {
            let table = view.diagram.tables.get(view.selected?)?;
            Some((table.schema.clone(), table.name.clone()))
        });
        let generation = state.request.generation;
        state.find.get_or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new("Find a table", cx));
            cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
                if *event == InputEvent::Submit {
                    let needle = input.read(cx).content().trim().to_owned();
                    if let Some(index) = this.diagram_matches(&needle).first().copied() {
                        this.select_diagram_card(index, true, cx);
                    }
                }
                cx.notify();
            })
            .detach();
            input
        });
        cx.spawn(async move |this, cx| {
            let live = session.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let read = live.schema_diagram(scope.as_deref(), &cancel)?;
                    let diagram = SchemaDiagram::build(read.reads);
                    let total = read.total_tables;
                    let incomplete = diagram.tables.len() < total;
                    let mut scene = build_scene(&diagram).ok_or("No tables with visible columns in this scope")?;
                    let placements = pending.unwrap_or_else(|| {
                        arrangement
                            .as_deref()
                            .map(placement::load)
                            .transpose()
                            .unwrap_or_else(|error| {
                                tracing::warn!(%error, "could not read the diagram layout");
                                None
                            })
                            .unwrap_or_default()
                    });
                    scene.restore(&diagram, &placements);
                    let selected = selection.and_then(|(schema, name)| diagram.position_of(schema.as_deref(), &name));
                    Ok::<_, String>(DiagramView {
                        diagram,
                        scene,
                        total,
                        incomplete,
                        zoom: 1.0,
                        needs_fit: true,
                        selected,
                        neighbours_only: false,
                        placements,
                        gesture: None,
                        saving_layout: false,
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.is_current_session(&session) || this.search.diagram.request.generation != generation {
                    return;
                }
                let state = &mut this.search.diagram;
                state.loading = false;
                match outcome {
                    Ok(view) => state.view = Some(view),
                    Err(error) => state.failure = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn diagram_matches(&self, needle: &str) -> Vec<usize> {
        let Some(view) = &self.search.diagram.view else {
            return Vec::new();
        };
        let needle = needle.to_lowercase();
        view.diagram
            .tables
            .iter()
            .enumerate()
            .filter(|(_, table)| needle.is_empty() || diagram_table_name(table).to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    }

    fn select_diagram_card(&mut self, index: usize, centre: bool, cx: &mut Context<Self>) {
        let Some(view) = &mut self.search.diagram.view else {
            return;
        };
        let Some(card) = view.scene.cards.get(index) else {
            return;
        };
        view.selected = Some(index);
        if centre {
            let bounds = self.search.diagram.scroll.bounds();
            let rect = card.rect();
            self.search.diagram.scroll.set_offset(point(
                (bounds.size.width / 2.0 - px_at((rect.x + rect.width / 2.0 - view.scene.extent.x) * view.zoom))
                    .min(px(0.0)),
                (bounds.size.height / 2.0 - px_at((rect.y + rect.height / 2.0 - view.scene.extent.y) * view.zoom))
                    .min(px(0.0)),
            ));
        }
        cx.notify();
    }

    fn begin_diagram_gesture(&mut self, index: usize, event: &MouseDownEvent, resize: bool, cx: &mut Context<Self>) {
        let Some(view) = &mut self.search.diagram.view else {
            return;
        };
        let Some(card) = view.scene.cards.get(index) else {
            return;
        };
        view.selected = Some(index);
        view.gesture = Some(CardGesture {
            index,
            pointer: event.position,
            origin: card.at,
            width: card.shape.width,
            resize,
        });
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn drag_diagram_card(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(view) = &mut self.search.diagram.view else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            view.gesture = None;
            return;
        }
        let Some(gesture) = &view.gesture else { return };
        let delta = event.position - gesture.pointer;
        let x = f64::from(f32::from(delta.x)) / view.zoom;
        let y = f64::from(f32::from(delta.y)) / view.zoom;
        let (at, width) = if gesture.resize {
            (gesture.origin, (gesture.width + x).clamp(120.0, 600.0))
        } else {
            (
                Point {
                    x: (gesture.origin.x + x).max(0.0),
                    y: (gesture.origin.y + y).max(0.0),
                },
                gesture.width,
            )
        };
        view.place_card(gesture.index, at, width);
        cx.notify();
    }

    pub(crate) fn end_diagram_gesture(&mut self) {
        if let Some(view) = &mut self.search.diagram.view {
            view.gesture = None;
        }
    }

    fn save_diagram_layout(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.search.diagram;
        let Some(view) = state.view.as_mut().filter(|view| !view.saving_layout) else {
            return;
        };
        let Some(path) = self
            .live
            .as_ref()
            .and_then(|live| live.diagram_layout_path(state.scope.as_deref()))
        else {
            self.status = "The diagram layout location is unavailable".into();
            cx.notify();
            return;
        };
        let placements = view.placements.clone();
        let generation = state.request.generation;
        view.saving_layout = true;
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { placement::save(&path, &placements) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.search.diagram;
                if state.request.generation != generation {
                    return;
                }
                if let Some(view) = &mut state.view {
                    view.saving_layout = false;
                }
                this.status = match outcome {
                    Ok(()) => "Saved diagram layout".into(),
                    Err(error) => format!("Could not save diagram layout: {error}"),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn reset_diagram_layout(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = &mut self.search.diagram.view
            && let Some(scene) = build_scene(&view.diagram)
        {
            view.scene = scene;
            view.needs_fit = true;
            view.placements.clear();
            self.search.diagram.scroll.set_offset(point(px(0.0), px(0.0)));
            cx.notify();
        }
    }

    fn diagram_scope_menu(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::DropdownMenu as _;
        let schemas: BTreeSet<_> = self
            .live
            .iter()
            .flat_map(|session| &session.tables)
            .filter_map(|table| table.schema.clone())
            .collect();
        let selected = self.search.diagram.scope.clone();
        let options: Vec<(gpui::SharedString, Option<String>)> = std::iter::once(None)
            .chain(schemas.into_iter().map(Some))
            .map(|scope| (scope.clone().unwrap_or_else(|| "All schemas".into()).into(), scope))
            .collect();
        let label = selected.clone().unwrap_or_else(|| "All schemas".into());
        let owner = cx.entity().downgrade();
        crate::surfaces::menu_button(
            "diagram-scope",
            crate::surfaces::MenuTrigger::new("Diagram schema scope", crate::surfaces::Bar::Compact)
                .icon("database")
                .label(label.as_str()),
            theme,
        )
        .dropdown_menu(move |menu, _, _| {
            let owner = owner.clone();
            crate::menus::select(menu, &options, Some(&selected), move |scope, _, cx| {
                let _ = owner.update(cx, |this, cx| this.load_schema_diagram(scope, cx));
            })
        })
    }

    fn diagram_viewport(&self, window: &Window) -> (f64, f64) {
        let sidebar = if self.sidebar.open {
            f64::from(self.sidebar.width)
        } else {
            0.0
        };
        let size = window.viewport_size();
        let width = (f64::from(f32::from(size.width)) - sidebar).max(0.0);
        let inspector = (width * 0.42).min(f64::from(size::SIDE_PANEL_WIDTH));
        (
            (width - inspector).max(200.0),
            (f64::from(f32::from(size.height)) - 150.0).max(200.0),
        )
    }

    pub(crate) fn prepare_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_set_cells_input(window, cx);
        self.focus_value_search_input(window, cx);
        self.focus_schema_diagram_input(window, cx);
        self.focus_filter_rule_input(window, cx);
        self.prepare_server_filter(cx);
        self.focus_backup_input(window, cx);
        self.focus_sql_file_input(window, cx);
        self.focus_tables_export_input(window, cx);
        self.observe_tab();
        self.apply_opened_sql(window, cx);
        let viewport = self.diagram_viewport(window);
        if let Some(view) = self.search.diagram.view.as_mut().filter(|view| view.needs_fit) {
            view.needs_fit = false;
            view.zoom = fit_zoom(&view.scene.extent, viewport);
        }
    }

    fn change_diagram_zoom(&mut self, change: impl FnOnce(f64, &Rect) -> f64, cx: &mut Context<Self>) {
        if let Some(view) = &mut self.search.diagram.view {
            view.zoom = zoom::clamp(change(view.zoom, &view.scene.extent));
        }
        cx.notify();
    }

    fn diagram_layout_menu(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::DropdownMenu as _;
        let owner = cx.entity().downgrade();
        let saving = self.search.diagram.view.as_ref().is_some_and(|view| view.saving_layout);
        crate::surfaces::menu_button(
            "diagram-layout",
            crate::surfaces::MenuTrigger::new("Save or reset the card layout", crate::surfaces::Bar::Compact)
                .icon("layout-grid")
                .label("Layout"),
            theme,
        )
        .dropdown_menu(move |menu, _, _| {
            let save = owner.clone();
            let reset = owner.clone();
            menu.item(
                crate::menus::item("Save layout", None, None, false)
                    .disabled(saving)
                    .on_click(move |_, _, cx| {
                        let _ = save.update(cx, |this, cx| this.save_diagram_layout(cx));
                    }),
            )
            .item(
                crate::menus::item("Reset layout", None, None, false).on_click(move |_, _, cx| {
                    let _ = reset.update(cx, |this, cx| this.reset_diagram_layout(cx));
                }),
            )
        })
    }

    fn diagram_toolbar(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let state = &self.search.diagram;
        let mut toolbar = div()
            .flex()
            .items_center()
            .gap_1()
            .p_2()
            .flex_none()
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(self.diagram_scope_menu(theme, cx));
        if let Some(view) = &state.view {
            toolbar = toolbar
                .child(crate::surfaces::truncated("diagram-count", diagram_count(view)).flex_1())
                .child(
                    compact_icon_button("diagram-zoom-out", "minus", "Zoom out", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.change_diagram_zoom(|z, _| zoom::step_out(z), cx))),
                )
                .child(div().child(format!("{}%", zoom::percent(view.zoom))))
                .child(
                    compact_icon_button("diagram-zoom-in", "plus", "Zoom in", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.change_diagram_zoom(|z, _| zoom::step_in(z), cx))),
                )
                .child(
                    compact_button(ButtonKind::Ghost, "diagram-fit", "Fit", theme).on_click(cx.listener(
                        |this, _, window, cx| {
                            let viewport = this.diagram_viewport(window);
                            this.change_diagram_zoom(|_, extent| fit_zoom(extent, viewport), cx);
                            this.search.diagram.scroll.set_offset(point(px(0.0), px(0.0)));
                        },
                    )),
                );
        }
        toolbar
            .children(state.view.as_ref().map(|_| self.diagram_layout_menu(theme, cx)))
            .children(self.diagram_can_draft().then(|| {
                compact_button(ButtonKind::Ghost, "diagram-create-table", "Create table", theme)
                    .on_click(cx.listener(|this, _, window, cx| this.begin_diagram_ddl(None, window, cx)))
            }))
            .child(
                compact_icon_button("diagram-refresh", "refresh-cw", "Refresh schema diagram", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.open_schema_diagram(cx))),
            )
            .child(
                compact_icon_button("diagram-export", "download", "Export Mermaid or DBML", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.open_schema_diagram_bar(cx))),
            )
    }

    pub(crate) fn diagram_page(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let state = &self.search.diagram;
        let message = match (&state.failure, state.loading) {
            (Some(failure), _) => Some(ListState::Failed(failure)),
            (None, true) => Some(ListState::Loading("Loading schema…")),
            (None, false) => None,
        };
        let body = if let Some(form) = self.diagram_ddl_editor(theme, cx) {
            form
        } else {
            match &state.view {
                Some(view) => div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("diagram-scroll")
                            .track_scroll(&state.scroll)
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_scroll()
                            .bg(rgb(theme.background))
                            .child(diagram_canvas(view, theme, cx)),
                    )
                    .child(self.diagram_inspector(view, theme, cx)),
                None => div().flex_1().min_h_0(),
            }
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .child(self.diagram_toolbar(theme, cx))
            .children(message.map(|state| list_state(state, theme).px_2().py_1()))
            .child(body)
    }

    fn diagram_inspector(&self, view: &DiagramView, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let find = self.search.diagram.find.as_ref();
        let input = find.map_or("", |input| input.read(cx).content());
        let searching = !input.trim().is_empty();
        let matches = self.diagram_matches(input.trim());
        let selected = view.selected.and_then(|index| view.diagram.tables.get(index));
        let mut inspector = div()
            .id("diagram-inspector")
            .flex_none()
            .flex()
            .flex_col()
            .min_h_0()
            .w(px(size::SIDE_PANEL_WIDTH))
            .max_w(gpui::relative(0.42))
            .p_2()
            .gap_2()
            .border_l_1()
            .border_color(rgb(theme.border))
            .children(find.cloned());
        if let Some(table) = selected {
            inspector = inspector.child(self.diagram_card_actions(view, table, theme, cx));
        }
        if let Some(draft) = self.diagram_relationship_editor(view, theme, cx) {
            return inspector.child(draft);
        }
        if !searching && let Some(table) = selected {
            return inspector.child(self.diagram_table_details(view, table, theme, cx));
        }
        inspector.child(
            div()
                .id("diagram-find-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(matches.into_iter().map(|index| {
                    let name = diagram_table_name(&view.diagram.tables[index]);
                    compact_button(ButtonKind::Ghost, ("diagram-find-item", index), name, theme)
                        .w_full()
                        .on_click(cx.listener(move |this, _, _, cx| this.select_diagram_card(index, true, cx)))
                })),
        )
    }

    fn diagram_table_details(
        &self,
        view: &DiagramView,
        table: &DiagramTable,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = view.selected;
        let relations = view.diagram.relations.iter().filter_map(|relation| {
            let (other, label) = if Some(relation.from) == selected {
                (relation.to, "References")
            } else if Some(relation.to) == selected {
                (relation.from, "Referenced by")
            } else {
                return None;
            };
            let table = view.diagram.tables.get(other)?;
            let title = format!("{label} {}", diagram_table_name(table));
            Some(
                compact_button(ButtonKind::Ghost, ("diagram-link", other), title, theme)
                    .tooltip(crate::surfaces::tip(relation.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| this.select_diagram_card(other, true, cx))),
            )
        });
        div()
            .id("diagram-table-details")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(crate::surfaces::section_header("Columns", theme))
            .children(table.columns.iter().enumerate().map(|(index, column)| {
                div()
                    .flex()
                    .gap_1()
                    .py_1()
                    .child(crate::surfaces::truncated(("diagram-column-name", index), column.name.clone()).flex_1())
                    .child(
                        crate::surfaces::truncated(("diagram-column-type", index), column.type_name.clone())
                            .text_color(rgb(theme.muted_foreground)),
                    )
            }))
            .child(crate::surfaces::section_header("Relationships", theme))
            .children(
                relations
                    .enumerate()
                    .map(|(index, control)| div().id(("diagram-link-row", index)).child(control)),
            )
    }

    fn diagram_card_actions(
        &self,
        view: &DiagramView,
        table: &DiagramTable,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = bookie_core::TableInfo {
            schema: table.schema.clone(),
            name: table.name.clone(),
        };
        let open_data = target.clone();
        let can_add_relationship = self.live.as_ref().is_some_and(|live| {
            live.writable.is_some() && bookie_core::sql_dialect::alters_foreign_keys(&live.driver_id)
        });
        let relationship_source = target.clone();
        let column_target = target.clone();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                crate::surfaces::truncated("diagram-selected-table", diagram_table_name(table))
                    .font_weight(gpui::FontWeight::MEDIUM),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        compact_button(ButtonKind::Ghost, "diagram-open-data", "Open data", theme).on_click(
                            cx.listener(move |this, _, window, cx| this.open_table_named(&open_data, window, cx)),
                        ),
                    )
                    .child(
                        compact_button(ButtonKind::Ghost, "diagram-open-structure", "Structure", theme).on_click(
                            cx.listener(move |this, _, window, cx| {
                                this.open_table_for(&target, crate::catalog_nav::TableOpenAction::Structure, window, cx)
                            }),
                        ),
                    )
                    .children(self.diagram_can_draft().then(|| {
                        compact_button(ButtonKind::Ghost, "diagram-add-column", "Add columns", theme).on_click(
                            cx.listener(move |this, _, window, cx| {
                                this.begin_diagram_ddl(Some(column_target.clone()), window, cx);
                            }),
                        )
                    }))
                    .children(can_add_relationship.then(|| {
                        compact_button(ButtonKind::Ghost, "diagram-add-relationship", "Add relationship", theme)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.begin_diagram_relationship(relationship_source.clone(), window, cx);
                            }))
                    }))
                    .child(
                        compact_button(
                            ButtonKind::Ghost,
                            "diagram-neighbours",
                            if view.neighbours_only { "Show all" } else { "Neighbours" },
                            theme,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(view) = &mut this.search.diagram.view {
                                view.neighbours_only = !view.neighbours_only;
                                cx.notify();
                            }
                        })),
                    ),
            )
    }
}

fn fit_zoom(extent: &Rect, viewport: (f64, f64)) -> f64 {
    zoom::fit((extent.width, extent.height), viewport)
}

fn diagram_table_name(table: &DiagramTable) -> String {
    table_label(&TableInfo {
        schema: table.schema.clone(),
        name: table.name.clone(),
    })
}

fn diagram_count(view: &DiagramView) -> String {
    if view.incomplete {
        format!("{} of {} tables shown", view.diagram.tables.len(), view.total)
    } else {
        format!("{} tables", view.total)
    }
}

fn visible_cards(view: &DiagramView) -> Option<BTreeSet<usize>> {
    let selected = view.selected.filter(|_| view.neighbours_only)?;
    let mut visible = BTreeSet::from([selected]);
    for relation in &view.diagram.relations {
        if relation.from == selected || relation.to == selected {
            visible.extend([relation.from, relation.to]);
        }
    }
    Some(visible)
}

fn diagram_canvas(view: &DiagramView, theme: Theme, cx: &mut Context<GridView>) -> Div {
    let (zoom, extent) = (view.zoom, view.scene.extent);
    let visible = visible_cards(view);
    let edges: Vec<_> = view
        .scene
        .edges
        .iter()
        .zip(&view.diagram.relations)
        .filter(|(_, relation)| {
            visible
                .as_ref()
                .is_none_or(|visible| visible.contains(&relation.from) && visible.contains(&relation.to))
        })
        .map(|(edge, _)| *edge)
        .collect();
    let color = rgb(theme.muted_foreground);
    let lines = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let at = |x: f64, y: f64| {
                point(
                    bounds.origin.x + px_at((x - extent.x) * zoom),
                    bounds.origin.y + px_at((y - extent.y) * zoom),
                )
            };
            for edge in &edges {
                let mut path = PathBuilder::stroke(px(DIAGRAM_EDGE_STROKE_WIDTH));
                path.move_to(at(edge.start.x, edge.start.y));
                path.line_to(at(edge.start_stub.x, edge.start_stub.y));
                path.cubic_bezier_to(
                    at(edge.end_stub.x, edge.end_stub.y),
                    at(edge.control_start.x, edge.control_start.y),
                    at(edge.control_end.x, edge.control_end.y),
                );
                path.line_to(at(edge.end.x, edge.end.y));
                if let Ok(built) = path.build() {
                    window.paint_path(built, color);
                }
            }
        },
    )
    .absolute()
    .size_full();
    let cards = view
        .scene
        .cards
        .iter()
        .zip(&view.diagram.tables)
        .enumerate()
        .filter(|(index, _)| visible.as_ref().is_none_or(|visible| visible.contains(index)))
        .map(|(index, (card, table))| {
            table_card(
                card,
                (index, table),
                (extent, zoom),
                view.selected == Some(index),
                theme,
                cx,
            )
        })
        .collect::<Vec<_>>();
    div()
        .relative()
        .w(px_at(extent.width * zoom))
        .h(px_at(extent.height * zoom))
        .child(lines)
        .children(cards)
}

fn card_rows(
    card: &bookie_app_services::er_diagram::scene::Card,
    table: &DiagramTable,
    zoom: f64,
    theme: Theme,
) -> Vec<Div> {
    table
        .columns
        .iter()
        .map(|column| {
            let marker = if column.primary_key {
                "PK"
            } else if column.foreign_key {
                "FK"
            } else {
                ""
            };
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .h(px_at(card.shape.row_height * zoom))
                .child(
                    div()
                        .w(px_at(24.0 * zoom))
                        .flex_none()
                        .whitespace_nowrap()
                        .text_color(rgb(theme.foreground))
                        .child(marker),
                )
                .child(div().flex_1().child(column.name.clone()))
                .child(
                    div()
                        .text_color(rgb(theme.muted_foreground))
                        .child(column.type_name.clone()),
                )
        })
        .collect()
}

fn table_card(
    card: &bookie_app_services::er_diagram::scene::Card,
    (index, table): (usize, &DiagramTable),
    (extent, zoom): (Rect, f64),
    selected: bool,
    theme: Theme,
    cx: &mut Context<GridView>,
) -> gpui::Stateful<Div> {
    let rect = card.rect();
    let rows = card_rows(card, table, zoom, theme);
    div()
        .id(("diagram-card", index))
        .cursor_pointer()
        .debug_selector(move || format!("diagram-card-{index}"))
        .on_click(cx.listener(move |this, _, _, cx| this.select_diagram_card(index, false, cx)))
        .absolute()
        .left(px_at((rect.x - extent.x) * zoom))
        .top(px_at((rect.y - extent.y) * zoom))
        .w(px_at(rect.width * zoom))
        .h(px_at(rect.height * zoom))
        .overflow_hidden()
        .text_size(px_at(12.0 * zoom))
        .bg(rgb(theme.chrome))
        .border_1()
        .border_color(rgb(if selected { theme.focus_ring } else { theme.border }))
        .rounded(px_at(f64::from(size::CONTROL_RADIUS) * zoom))
        .child(
            div()
                .flex()
                .items_center()
                .px_2()
                .h(px_at(card.shape.header_height * zoom))
                .font_weight(gpui::FontWeight::MEDIUM)
                .bg(rgb(theme.muted))
                .cursor(gpui::CursorStyle::OpenHand)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event, _, cx| {
                        this.begin_diagram_gesture(index, event, false, cx);
                    }),
                )
                .child(crate::surfaces::truncated(("diagram-card-name", index), diagram_table_name(table)).flex_1())
                .child(
                    div()
                        .flex_none()
                        .size(px(size::SM))
                        .bg(rgb(theme.muted_foreground))
                        .cursor(gpui::CursorStyle::ResizeLeftRight)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event, _, cx| {
                                this.begin_diagram_gesture(index, event, true, cx);
                            }),
                        ),
                ),
        )
        .children(rows)
}
