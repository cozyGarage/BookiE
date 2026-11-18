use bookie_gpui::messages::MessageLog;
use bookie_gpui::shell::ResultView;

pub(crate) struct PanelState {
    pub view: ResultView,
    pub messages: MessageLog,
    pub ratio: f32,
    pub open: bool,
    pub resizing: bool,
}

impl PanelState {
    pub fn new(ratio: f32) -> Self {
        Self {
            view: ResultView::Table,
            messages: MessageLog::default(),
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
    pub fn new(settings: bookie_gpui::font_settings::FontSettings, current: gpui::SharedString, names: Vec<String>) -> Self {
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
}
