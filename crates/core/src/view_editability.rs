#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewEditability {
    NotApplicable,
    Editable {
        key_columns: Vec<String>,
        read_only_columns: Vec<String>,
        insertable: bool,
    },
    NotUpdatable,
    KeyNotExposed,
}
