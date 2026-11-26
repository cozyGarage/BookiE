use bookie_app_services::er_diagram::SchemaDiagram;
use bookie_app_services::er_diagram::layout::{Point, Size, arrange};
use bookie_app_services::er_diagram::placement::CardPlacement;
use bookie_app_services::er_diagram::scene::{Card, CardShape, EdgePath, Rect, extent, route};

pub const CARD_WIDTH: f64 = 230.0;
pub const HEADER_HEIGHT: f64 = 30.0;
pub const ROW_HEIGHT: f64 = 22.0;

pub struct Scene {
    pub cards: Vec<Card>,
    pub edges: Vec<EdgePath>,
    pub extent: Rect,
}

impl Scene {
    pub fn place_card(&mut self, diagram: &SchemaDiagram, index: usize, at: Point, width: f64) {
        let Some(card) = self.cards.get_mut(index) else { return };
        card.at = at;
        card.shape.width = width;
        for (edge, relation) in self.edges.iter_mut().zip(&diagram.relations) {
            if relation.from != index && relation.to != index {
                continue;
            }
            if let (Some(from), Some(to)) = (self.cards.get(relation.from), self.cards.get(relation.to)) {
                *edge = route(from, &relation.from_columns, to, &relation.to_columns);
            }
        }
        if let Some(bounds) = extent(&self.cards, &self.edges) {
            self.extent.width = self.extent.width.max(bounds.right() - self.extent.x);
            self.extent.height = self.extent.height.max(bounds.bottom() - self.extent.y);
        }
    }

    pub fn restore(&mut self, diagram: &SchemaDiagram, placements: &[CardPlacement]) {
        for placement in placements {
            if let Some(index) = diagram.position_of(placement.schema.as_deref(), &placement.table) {
                let at = Point {
                    x: placement.x,
                    y: placement.y,
                };
                self.place_card(diagram, index, at, placement.width);
            }
        }
    }
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
