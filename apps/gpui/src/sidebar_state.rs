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
    pub narrow_handled: bool,
    pub scroll: gpui::ScrollHandle,
    pub reveal: std::cell::RefCell<Option<bookie_core::TableInfo>>,
}

impl SidebarState {
    pub const DEFAULT_WIDTH: f32 = 340.0;
    pub const NARROW_WINDOW: f32 = 760.0;

    pub fn collapse_when_narrow(&mut self, window_width: f32) {
        let (handled, collapse) = narrow_step(window_width, self.narrow_handled);
        self.narrow_handled = handled;
        if collapse {
            self.open = false;
        }
    }

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
            narrow_handled: false,
            scroll: gpui::ScrollHandle::new(),
            reveal: std::cell::RefCell::new(None),
        }
    }
}

fn narrow_step(window_width: f32, handled: bool) -> (bool, bool) {
    if window_width >= SidebarState::NARROW_WINDOW {
        (false, false)
    } else {
        (true, !handled)
    }
}

#[cfg(test)]
mod narrow_tests {
    use super::narrow_step;

    #[test]
    fn a_narrow_window_collapses_once_and_a_wide_one_rearms() {
        assert_eq!(narrow_step(700.0, false), (true, true));
        assert_eq!(narrow_step(700.0, true), (true, false));
        assert_eq!(narrow_step(1100.0, true), (false, false));
        assert_eq!(narrow_step(700.0, false), (true, true));
    }
}
