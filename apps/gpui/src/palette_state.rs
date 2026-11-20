use gpui::Entity;

use crate::text_input::TextInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteMode {
    Commands,
    Tables,
    Columns,
    Shortcuts,
    Favorites,
    DeleteFavorite,
}

impl PaletteMode {
    pub fn placeholder(self) -> &'static str {
        match self {
            PaletteMode::Commands => "Type a command",
            PaletteMode::Tables => "Go to a table or favorite",
            PaletteMode::Columns => "Jump to a column",
            PaletteMode::Shortcuts => "Search keyboard shortcuts",
            PaletteMode::Favorites => "Open a favorite",
            PaletteMode::DeleteFavorite => "Delete a favorite",
        }
    }
}

pub(crate) struct PaletteState {
    pub input: Entity<TextInput>,
    pub open: bool,
    pub index: usize,
    pub fonts: bool,
    pub mode: PaletteMode,
    pub address: Option<String>,
    pub pattern: Option<String>,
    pub follow: Option<String>,
    pub delimiter: Option<bookie_gpui::csv_delimiter::Parsed>,
    pub limit: Option<bookie_gpui::export_limit::Parsed>,
}

impl PaletteState {
    pub fn new(input: Entity<TextInput>) -> Self {
        Self {
            input,
            open: false,
            index: 0,
            fonts: false,
            mode: PaletteMode::Commands,
            address: None,
            pattern: None,
            follow: None,
            delimiter: None,
            limit: None,
        }
    }
}
