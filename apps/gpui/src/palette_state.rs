use gpui::Entity;

use crate::text_input::TextInput;

pub(crate) struct PaletteState {
    pub input: Entity<TextInput>,
    pub open: bool,
    pub index: usize,
    pub fonts: bool,
    pub address: Option<String>,
    pub pattern: Option<String>,
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
            address: None,
            pattern: None,
            delimiter: None,
            limit: None,
        }
    }
}
