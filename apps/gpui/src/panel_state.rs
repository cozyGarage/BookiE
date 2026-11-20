use bookie_app_services::explain_plan::PlanNode;
use bookie_gpui::messages::MessageLog;
use bookie_gpui::shell::ResultView;

pub(crate) struct PanelState {
    pub view: ResultView,
    pub messages: MessageLog,
    pub plan: Vec<PlanNode>,
    pub explaining: bool,
    pub ratio: f32,
    pub open: bool,
    pub resizing: bool,
}

impl PanelState {
    pub fn new(ratio: f32) -> Self {
        Self {
            view: ResultView::Table,
            messages: MessageLog::default(),
            plan: Vec::new(),
            explaining: false,
            ratio,
            open: true,
            resizing: false,
        }
    }
}

pub(crate) struct FontState {
    pub settings: bookie_gpui::font_settings::FontSettings,
    pub current: gpui::SharedString,
    pub names: Vec<String>,
}

impl FontState {
    pub fn new(
        settings: bookie_gpui::font_settings::FontSettings,
        current: gpui::SharedString,
        names: Vec<String>,
    ) -> Self {
        Self {
            settings,
            current,
            names,
        }
    }
}

#[derive(Default)]
pub(crate) struct ExportState {
    pub csv_delimiter: Option<bookie_core::export::CustomDelimiter>,
    pub limit: Option<usize>,
    pub file: crate::export_file_ui::ExportFileState,
}

pub(crate) struct SearchState {
    pub find: gpui::Entity<crate::text_input::TextInput>,
    pub filter: gpui::Entity<crate::text_input::TextInput>,
    pub filter_visible: bool,
    pub cursor: crate::MatchCursor,
    pub capped: bool,
    pub reset: bool,
    pub values: crate::value_search_ui::ValueSearchState,
    pub diagram: crate::schema_diagram_ui::SchemaDiagramState,
    pub rule: crate::filter_rule_ui::FilterRuleState,
    pub backup: crate::backup_ui::BackupState,
    pub sql_file: crate::sql_file_ui::SqlFileState,
    pub closed: crate::closed_tabs_ui::ClosedTabs,
    pub recent: crate::closed_tabs_ui::TabRecency,
    pub origin: Option<bookie_gpui::live::Origin>,
    pub tables_export: crate::tables_export_ui::TablesExportState,
}

impl SearchState {
    pub fn new(
        find: gpui::Entity<crate::text_input::TextInput>,
        filter: gpui::Entity<crate::text_input::TextInput>,
    ) -> Self {
        Self {
            find,
            filter,
            filter_visible: false,
            cursor: crate::MatchCursor::default(),
            capped: false,
            reset: false,
            values: Default::default(),
            diagram: Default::default(),
            rule: Default::default(),
            backup: Default::default(),
            sql_file: Default::default(),
            closed: Default::default(),
            recent: Default::default(),
            origin: None,
            tables_export: Default::default(),
        }
    }
}

pub(crate) struct CellEditState {
    pub editor: gpui::Entity<crate::text_input::TextInput>,
    pub active: Option<crate::grid_edit::Editing>,
    pub edits: crate::grid_edit::Edits,
    pub table: crate::grid_edit::Edits,
    pub undo: Vec<crate::edit_history::EditOp>,
    pub redo: Vec<crate::edit_history::EditOp>,
    pub set: crate::set_cells_ui::SetCellsState,
}

impl CellEditState {
    pub fn new(editor: gpui::Entity<crate::text_input::TextInput>) -> Self {
        Self {
            editor,
            active: None,
            edits: Default::default(),
            table: Default::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            set: Default::default(),
        }
    }
}

pub(crate) struct RunTabs {
    pub slots: Vec<crate::runs::RunSlot>,
    pub active: usize,
    pub scroll: gpui::ScrollHandle,
    pub next_number: usize,
}

impl RunTabs {
    pub fn new(first: crate::runs::RunSlot) -> Self {
        Self {
            slots: vec![first],
            active: 0,
            scroll: gpui::ScrollHandle::new(),
            next_number: 2,
        }
    }
}
