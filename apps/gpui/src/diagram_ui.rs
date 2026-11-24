use bookie_app_services::er_diagram::scene::Rect;
use bookie_app_services::er_diagram::zoom;
use bookie_app_services::er_diagram::{DiagramTable, SchemaDiagram};
use gpui::{PathBuilder, canvas, point};

use super::*;
use crate::chrome::ghost;
use crate::diagram_scene::{Scene, build_scene};

const DIAGRAM_EDGE_STROKE_WIDTH: f32 = 1.5;

pub(crate) struct DiagramView {
    pub diagram: SchemaDiagram,
    pub scene: Scene,
    pub zoom: f64,
    pub needs_fit: bool,
}

fn px_at(value: f64) -> gpui::Pixels {
    px(value as f32)
}

impl GridView {
    pub(crate) fn diagram_actions(&self) -> Vec<Action> {
        let enabled = self.live.as_ref().is_some_and(|session| !session.tables.is_empty());
        vec![Action {
            id: "show-schema-diagram",
            label: "Show the schema diagram".to_string(),
            enabled,
        }]
    }

    pub(crate) fn open_schema_diagram(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.live.clone() else {
            self.status = "Connect to a database to draw its schema".into();
            return;
        };
        self.status = "Reading every table…".into();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| session.schema_diagram(&cancel),
            |this, result| match result {
                Ok(diagram) => match build_scene(&diagram) {
                    Some(scene) => {
                        let zoom = fit_zoom(&scene.extent, (900.0, 600.0));
                        this.status = format!("Schema diagram of {} table(s)", diagram.tables.len());
                        this.search.diagram.view = Some(DiagramView {
                            diagram,
                            scene,
                            zoom,
                            needs_fit: true,
                        });
                        this.tabs.open_diagram();
                        this.sync_grid_owner();
                    }
                    None => this.status = "There are no tables with columns to draw".into(),
                },
                Err(message) => this.status = format!("Could not read the schema: {message}"),
            },
        );
    }

    fn diagram_viewport(&self, window: &Window) -> (f64, f64) {
        let sidebar = if self.sidebar.open {
            f64::from(self.sidebar.width)
        } else {
            0.0
        };
        let size = window.viewport_size();
        (
            (f64::from(f32::from(size.width)) - sidebar - 24.0).max(200.0),
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

    pub(crate) fn diagram_page(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let Some(view) = &self.search.diagram.view else {
            return self.placeholder_page("the schema diagram", theme);
        };
        let percent = zoom::percent(view.zoom);
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .flex_none()
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(format!("Schema diagram · {} tables", view.diagram.tables.len())),
            )
            .child(div().flex_1())
            .child(
                ghost("diagram-zoom-out", "Zoom out", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.change_diagram_zoom(|z, _| zoom::step_out(z), cx))),
            )
            .child(div().child(format!("{percent}%")))
            .child(
                ghost("diagram-zoom-in", "Zoom in", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.change_diagram_zoom(|z, _| zoom::step_in(z), cx))),
            )
            .child(
                ghost("diagram-fit", "Fit", theme).on_click(cx.listener(|this, _, window, cx| {
                    let viewport = this.diagram_viewport(window);
                    this.change_diagram_zoom(|_, extent| fit_zoom(extent, viewport), cx)
                })),
            );
        div().flex_1().flex().flex_col().min_h_0().child(header).child(
            div()
                .id("diagram-scroll")
                .flex_1()
                .min_h_0()
                .overflow_scroll()
                .bg(rgb(theme.background))
                .child(diagram_canvas(view, theme, cx)),
        )
    }
}

fn fit_zoom(extent: &Rect, viewport: (f64, f64)) -> f64 {
    zoom::fit((extent.width, extent.height), viewport)
}

fn diagram_canvas(view: &DiagramView, theme: Theme, cx: &mut Context<GridView>) -> Div {
    let (zoom, extent) = (view.zoom, view.scene.extent);
    let edges = view.scene.edges.clone();
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
        .map(|(index, (card, table))| table_card(card, (index, table), (extent, zoom), theme, cx))
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
                        .text_color(rgb(theme.info))
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
    theme: Theme,
    cx: &mut Context<GridView>,
) -> gpui::Stateful<Div> {
    let target = bookie_core::TableInfo {
        schema: table.schema.clone(),
        name: table.name.clone(),
    };
    let rect = card.rect();
    let rows = card_rows(card, table, zoom, theme);
    div()
        .id(("diagram-card", index))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, window, cx| this.open_table_named(&target, window, cx)))
        .absolute()
        .left(px_at((rect.x - extent.x) * zoom))
        .top(px_at((rect.y - extent.y) * zoom))
        .w(px_at(rect.width * zoom))
        .h(px_at(rect.height * zoom))
        .overflow_hidden()
        .text_size(px_at(12.0 * zoom))
        .bg(rgb(theme.chrome))
        .border_1()
        .border_color(rgb(theme.border))
        .rounded(px_at(4.0 * zoom))
        .child(
            div()
                .flex()
                .items_center()
                .px_2()
                .h(px_at(card.shape.header_height * zoom))
                .font_weight(gpui::FontWeight::MEDIUM)
                .bg(rgb(theme.muted))
                .child(match &table.schema {
                    Some(schema) => format!("{schema}.{}", table.name),
                    None => table.name.clone(),
                }),
        )
        .children(rows)
}
