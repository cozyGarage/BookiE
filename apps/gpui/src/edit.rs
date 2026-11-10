use crate::model::Cell;

pub const EDITABLE_COLUMNS: std::ops::Range<usize> = 1..4;

pub fn is_editable(column: usize, cell: &Cell) -> bool {
    EDITABLE_COLUMNS.contains(&column)
        && !matches!(cell, Cell::Undecodable(_))
        && !matches!(cell, Cell::Exact(value) if matches!(value.as_ref(), bookie_core::Value::Undecodable(_)))
}

pub fn parse_edit(kind: &str, text: &str) -> Result<Cell, String> {
    crate::core_bridge::parse_edit(kind, text)
}
