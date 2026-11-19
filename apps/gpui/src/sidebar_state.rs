use gpui::Entity;

use crate::text_input::TextInput;

pub(crate) struct FieldRow {
    pub name: String,
    pub data_type: String,
    pub primary_key: bool,
}

pub(crate) enum FieldState {
    Loading,
    Loaded(Vec<FieldRow>),
    Failed(String),
}

pub(crate) enum ObjectState {
    Loading,
    Loaded(Vec<bookie_core::CatalogObject>),
    Failed(String),
}

pub(crate) struct SidebarState {
    pub catalog_open: bool,
    pub catalog: Vec<(bookie_core::CatalogObjectKind, ObjectState)>,
    pub fields: Vec<(bookie_core::TableInfo, FieldState)>,
    pub open: bool,
    pub width: f32,
    pub resizing: bool,
    pub regex: bool,
    pub active_only: bool,
    pub search: Entity<TextInput>,
    pub import_menu_open: bool,
    pub connection_open: bool,
    pub database_open: bool,
    pub collapsed_schemas: bookie_results::sidebar_tree::CollapseState,
}

impl SidebarState {
    pub const DEFAULT_WIDTH: f32 = 340.0;

    pub fn new(search: Entity<TextInput>) -> Self {
        Self {
            catalog_open: true,
            catalog: Vec::new(),
            fields: Vec::new(),
            open: true,
            width: Self::DEFAULT_WIDTH,
            resizing: false,
            regex: false,
            active_only: false,
            search,
            import_menu_open: false,
            connection_open: true,
            database_open: true,
            collapsed_schemas: Default::default(),
        }
    }
}
