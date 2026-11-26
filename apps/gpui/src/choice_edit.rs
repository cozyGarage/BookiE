use bookie_app_services::enum_input::declared_member_labels;
use bookie_core::ColumnInfo;
use bookie_gpui::model::Cell;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Choice {
    Toggle(Cell),
    Members(Vec<String>),
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
    declared_member_labels(declared.trim())
        .filter(|labels| !labels.is_empty())
        .map(Choice::Members)
}

pub(crate) fn member_cell(label: &str, info: &ColumnInfo) -> Result<Cell, String> {
    if declared_member_labels(info.data_type.trim()).is_some_and(|labels| labels.iter().any(|member| member == label)) {
        Ok(Cell::Text(label.to_owned()))
    } else {
        Err(format!("{label} is not a declared value of {}", info.name))
    }
}
