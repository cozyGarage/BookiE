use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString, prelude::*};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        const ICONS: &[(&str, &[u8])] = &[
            $((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")))),*
        ];
    };
}

icons!(
    "arrow-left-right",
    "chevron-down",
    "chevron-right",
    "chevrons-right",
    "arrow-down-up",
    "folder-plus",
    "refresh-cw",
    "chevrons-left",
    "clock",
    "command",
    "database",
    "file-plus",
    "download",
    "folder",
    "house",
    "moon",
    "package",
    "palette",
    "panel-left",
    "pin",
    "play",
    "plus",
    "puzzle",
    "search",
    "sun",
    "table",
    "type",
    "x",
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}

pub fn icon(name: &'static str, color: u32) -> gpui::Svg {
    gpui::svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(gpui::px(15.0))
        .flex_none()
        .text_color(gpui::rgb(color))
}
