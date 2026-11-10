use crate::model::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Integer,
    Numeric,
    String,
    Boolean,
    Temporal,
    Structured,
    Identifier,
    Binary,
}

pub fn type_kind(cell: &Cell) -> Option<TypeKind> {
    match cell {
        Cell::Int(_) => Some(TypeKind::Integer),
        Cell::Float(_) | Cell::Decimal(_) => Some(TypeKind::Numeric),
        Cell::Text(_) => Some(TypeKind::String),
        Cell::Bool(_) => Some(TypeKind::Boolean),
        Cell::Date(_) | Cell::Time(_) | Cell::DateTime(_) | Cell::TimestampTz(_) => Some(TypeKind::Temporal),
        Cell::Json(_) | Cell::JsonText(_) => Some(TypeKind::Structured),
        Cell::Uuid(_) => Some(TypeKind::Identifier),
        Cell::Bytes(_) => Some(TypeKind::Binary),
        Cell::Null | Cell::Undecodable(_) => None,
    }
}

pub fn type_color(kind: TypeKind, dark: bool) -> u32 {
    let (light, dark_color) = match kind {
        TypeKind::Integer => (0x1d4ed8, 0x93c5fd),
        TypeKind::Numeric => (0x0e7490, 0x67e8f9),
        TypeKind::String => (0x166534, 0x86efac),
        TypeKind::Boolean => (0xc2410c, 0xfdba74),
        TypeKind::Temporal => (0x7e22ce, 0xd8b4fe),
        TypeKind::Structured => (0xbe185d, 0xf9a8d4),
        TypeKind::Identifier => (0x92400e, 0xfcd34d),
        TypeKind::Binary => (0xb91c1c, 0xfca5a5),
    };
    if dark { dark_color } else { light }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cell_kind_maps_to_one_colour_class_and_null_stays_neutral() {
        let cases = [
            (Cell::Int(1), Some(TypeKind::Integer)),
            (Cell::Float(1.5), Some(TypeKind::Numeric)),
            (Cell::Decimal("1.50".into()), Some(TypeKind::Numeric)),
            (Cell::Text("a".into()), Some(TypeKind::String)),
            (Cell::Bool(true), Some(TypeKind::Boolean)),
            (Cell::Date("2026-11-10".into()), Some(TypeKind::Temporal)),
            (
                Cell::TimestampTz("2026-11-10T00:00:00Z".into()),
                Some(TypeKind::Temporal),
            ),
            (Cell::Json("{}".into()), Some(TypeKind::Structured)),
            (Cell::Uuid("9f8e".into()), Some(TypeKind::Identifier)),
            (Cell::Bytes(vec![1]), Some(TypeKind::Binary)),
            (Cell::Null, None),
            (Cell::Undecodable("numeric".into()), None),
        ];
        for (cell, expected) in cases {
            assert_eq!(type_kind(&cell), expected, "{cell:?}");
        }
    }

    #[test]
    fn the_light_and_dark_palettes_differ_for_every_kind() {
        for kind in [
            TypeKind::Integer,
            TypeKind::Numeric,
            TypeKind::String,
            TypeKind::Boolean,
            TypeKind::Temporal,
            TypeKind::Structured,
            TypeKind::Identifier,
            TypeKind::Binary,
        ] {
            assert_ne!(type_color(kind, false), type_color(kind, true), "{kind:?}");
        }
        assert_eq!(type_color(TypeKind::Integer, false), 0x1d4ed8);
        assert_eq!(type_color(TypeKind::Integer, true), 0x93c5fd);
    }
}
