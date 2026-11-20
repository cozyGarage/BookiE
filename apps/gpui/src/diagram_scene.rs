use bookie_app_services::er_diagram::SchemaDiagram;
use bookie_app_services::er_diagram::layout::{Size, arrange};
use bookie_app_services::er_diagram::scene::{Card, CardShape, EdgePath, Rect, extent, route};

pub const CARD_WIDTH: f64 = 230.0;
pub const HEADER_HEIGHT: f64 = 30.0;
pub const ROW_HEIGHT: f64 = 22.0;

pub struct Scene {
    pub cards: Vec<Card>,
    pub edges: Vec<EdgePath>,
    pub extent: Rect,
}

pub fn build_scene(diagram: &SchemaDiagram) -> Option<Scene> {
    let shapes: Vec<CardShape> = diagram
        .tables
        .iter()
        .map(|table| CardShape {
            width: CARD_WIDTH,
            header_height: HEADER_HEIGHT,
            row_height: ROW_HEIGHT,
            rows: table.columns.len(),
            footer: 0.0,
        })
        .collect();
    let sizes: Vec<Size> = shapes.iter().map(CardShape::size).collect();
    let links: Vec<(usize, usize)> = diagram
        .relations
        .iter()
        .map(|relation| (relation.from, relation.to))
        .collect();
    let cards: Vec<Card> = arrange(&sizes, &links)
        .into_iter()
        .zip(shapes)
        .map(|(at, shape)| Card { at, shape })
        .collect();
    let edges: Vec<EdgePath> = diagram
        .relations
        .iter()
        .filter_map(|relation| {
            let (from, to) = (cards.get(relation.from)?, cards.get(relation.to)?);
            Some(route(from, &relation.from_columns, to, &relation.to_columns))
        })
        .collect();
    let extent = extent(&cards, &edges)?;
    Some(Scene { cards, edges, extent })
}

#[cfg(test)]
#[path = "diagram_scene_tests.rs"]
mod tests;
