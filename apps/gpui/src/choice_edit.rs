use bookie_app_services::enum_input::declared_member_labels;
use bookie_core::{ColumnInfo, QualifiedTypeName};
use bookie_gpui::model::Cell;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Choice {
    Toggle(Cell),
    Members(Vec<String>),
    PostgresEnum(QualifiedTypeName),
}

fn is_boolean_type(data_type: &str) -> bool {
    let kind = data_type.trim();
    kind.eq_ignore_ascii_case("bool") || kind.eq_ignore_ascii_case("boolean")
}

fn is_member_type(data_type: &str) -> bool {
    let kind = data_type.trim().to_ascii_lowercase();
    kind.starts_with("enum") || kind.starts_with("set")
}

pub(crate) fn is_choice_candidate(data_type: &str, cell: &Cell) -> bool {
    matches!(cell, Cell::Bool(_)) || is_boolean_type(data_type) || is_member_type(data_type)
}

pub(crate) fn next_boolean(current: &Cell, nullable: bool) -> Cell {
    match current {
        Cell::Bool(true) => Cell::Bool(false),
        Cell::Bool(false) if nullable => Cell::Null,
        _ => Cell::Bool(true),
    }
}

pub(crate) fn choice_for(data_type: &str, info: Option<&ColumnInfo>, cell: &Cell) -> Option<Choice> {
    let declared = info.map_or(data_type, |info| info.data_type.as_str());
    let boolean =
        matches!(cell, Cell::Bool(_) | Cell::Null) && (is_boolean_type(data_type) || is_boolean_type(declared));
    if matches!(cell, Cell::Bool(_)) || boolean {
        let nullable = info.is_some_and(|info| info.nullable);
        return Some(Choice::Toggle(next_boolean(cell, nullable)));
    }
    if let Some(type_name) = info.and_then(|info| info.enum_type.as_ref()) {
        return (matches!(cell, Cell::Text(_) | Cell::Null) && !declared.ends_with("[]"))
            .then(|| Choice::PostgresEnum(type_name.clone()));
    }
    declared_member_labels(declared.trim())
        .filter(|labels| !labels.is_empty())
        .map(Choice::Members)
}

pub(crate) fn member_cell(label: &str, labels: &[String]) -> Option<Cell> {
    labels
        .iter()
        .any(|member| member == label)
        .then(|| Cell::Text(label.to_owned()))
}
