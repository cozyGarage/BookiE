use serde::{Deserialize, Serialize};

pub const DEFAULT_PAGE_SIZE: u64 = 1_000;
pub const MIN_PAGE_SIZE: u64 = 1;
pub const MAX_PAGE_SIZE: u64 = bookie_core::MAX_QUERY_CURSOR_PAGE_ROWS as u64;
pub const MIN_TYPOGRAPHY_SIZE: u32 = 8;
pub const MAX_TYPOGRAPHY_SIZE: u32 = 32;
pub const MAX_GRID_TYPOGRAPHY_SIZE: u32 = 24;

pub fn bounded_page_size(page_size: u64) -> u64 {
    if page_size == 0 {
        DEFAULT_PAGE_SIZE
    } else {
        page_size.min(MAX_PAGE_SIZE)
    }
}

pub fn bounded_typography_size(size: u32) -> u32 {
    if size == 0 {
        0
    } else {
        size.clamp(MIN_TYPOGRAPHY_SIZE, MAX_TYPOGRAPHY_SIZE)
    }
}

pub fn bounded_typography_weight(weight: u32) -> u32 {
    if weight == 0 {
        0
    } else {
        (weight.saturating_add(50) / 100 * 100).clamp(100, 900)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TypographyOverride {
    #[serde(default)]
    pub font_family: String,
    #[serde(default)]
    pub font_size: u32,
    #[serde(default)]
    pub font_weight: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    pub default_page_size: u64,
    pub confirm_destructive: bool,
    pub editor_font_size: u32,
    #[serde(default = "default_history_retention_days")]
    pub history_retention_days: u32,
    #[serde(default = "default_query_timeout_secs")]
    pub query_timeout_secs: u32,
    #[serde(default = "default_csv_include_header")]
    pub csv_include_header: bool,
    #[serde(default)]
    pub color_scheme: ColorSchemePref,
    #[serde(default)]
    pub theme: ThemePref,
    #[serde(default)]
    pub editor_font_family: String,
    #[serde(default)]
    pub editor_vim_mode: bool,
    #[serde(default)]
    pub null_display: NullDisplayPref,
    #[serde(default)]
    pub date_format: DateFormatPref,
    #[serde(default = "default_show_invisible_characters")]
    pub show_invisible_characters: bool,
    #[serde(default = "default_open_tables_in_preview_tab")]
    pub open_tables_in_preview_tab: bool,
    #[serde(default)]
    pub ui_typography: TypographyOverride,
    #[serde(default)]
    pub editor_font_weight: u32,
    #[serde(default)]
    pub grid_typography: TypographyOverride,
    #[serde(default)]
    pub output_typography: TypographyOverride,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NullDisplayPref {
    #[default]
    Standard,
    Symbol,
    Parenthesized,
}

impl NullDisplayPref {
    pub const ALL: [Self; 3] = [Self::Standard, Self::Symbol, Self::Parenthesized];

    pub fn key(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Symbol => "symbol",
            Self::Parenthesized => "parenthesized",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|style| style.key() == key)
            .unwrap_or_default()
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|style| *style == self).unwrap_or(0) as u32
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DateFormatPref {
    #[default]
    Iso,
    IsoDate,
    UsLong,
    UsShort,
    EuLong,
    EuShort,
}

impl DateFormatPref {
    pub const ALL: [Self; 6] = [
        Self::Iso,
        Self::IsoDate,
        Self::UsLong,
        Self::UsShort,
        Self::EuLong,
        Self::EuShort,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Iso => "iso",
            Self::IsoDate => "iso-date",
            Self::UsLong => "us-long",
            Self::UsShort => "us-short",
            Self::EuLong => "eu-long",
            Self::EuShort => "eu-short",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|format| format.key() == key)
            .unwrap_or_default()
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|format| *format == self).unwrap_or(0) as u32
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorSchemePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ColorSchemePref {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|scheme| scheme.key() == key)
            .unwrap_or_default()
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|scheme| *scheme == self).unwrap_or(0) as u32
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    #[default]
    Default,
    Ledger,
    Catppuccin,
    Tokyo,
    Gruvbox,
}

impl ThemePref {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Ledger,
        Self::Catppuccin,
        Self::Tokyo,
        Self::Gruvbox,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Ledger => "ledger",
            Self::Catppuccin => "catppuccin",
            Self::Tokyo => "tokyo",
            Self::Gruvbox => "gruvbox",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|theme| theme.key() == key)
            .unwrap_or_default()
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|theme| *theme == self).unwrap_or(0) as u32
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or_default()
    }
}

fn default_history_retention_days() -> u32 {
    30
}

fn default_query_timeout_secs() -> u32 {
    60
}

fn default_csv_include_header() -> bool {
    true
}

fn default_open_tables_in_preview_tab() -> bool {
    true
}

fn default_show_invisible_characters() -> bool {
    true
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            default_page_size: DEFAULT_PAGE_SIZE,
            confirm_destructive: true,
            editor_font_size: 0,
            history_retention_days: default_history_retention_days(),
            query_timeout_secs: default_query_timeout_secs(),
            csv_include_header: default_csv_include_header(),
            color_scheme: ColorSchemePref::default(),
            theme: ThemePref::default(),
            editor_font_family: String::new(),
            editor_vim_mode: false,
            null_display: NullDisplayPref::default(),
            date_format: DateFormatPref::default(),
            show_invisible_characters: default_show_invisible_characters(),
            open_tables_in_preview_tab: default_open_tables_in_preview_tab(),
            ui_typography: TypographyOverride::default(),
            editor_font_weight: 0,
            grid_typography: TypographyOverride::default(),
            output_typography: TypographyOverride::default(),
        }
    }
}

#[cfg(test)]
mod page_size_tests {
    use super::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, MIN_PAGE_SIZE, bounded_page_size};

    #[test]
    fn page_size_bounds_preserve_custom_values_and_repair_invalid_ones() {
        assert_eq!(bounded_page_size(MIN_PAGE_SIZE), MIN_PAGE_SIZE);
        assert_eq!(bounded_page_size(2_347), 2_347);
        assert_eq!(bounded_page_size(MAX_PAGE_SIZE), MAX_PAGE_SIZE);
        assert_eq!(bounded_page_size(0), DEFAULT_PAGE_SIZE);
        assert_eq!(bounded_page_size(MAX_PAGE_SIZE + 1), MAX_PAGE_SIZE);
    }
}

#[cfg(test)]
mod typography_tests {
    use super::*;

    #[test]
    fn typography_size_and_weight_keep_system_defaults_and_bound_overrides() {
        assert_eq!(bounded_typography_size(0), 0);
        assert_eq!(bounded_typography_size(1), MIN_TYPOGRAPHY_SIZE);
        assert_eq!(bounded_typography_size(20), 20);
        assert_eq!(bounded_typography_size(99), MAX_TYPOGRAPHY_SIZE);
        assert_eq!(bounded_typography_weight(0), 0);
        assert_eq!(bounded_typography_weight(1), 100);
        assert_eq!(bounded_typography_weight(650), 700);
        assert_eq!(bounded_typography_weight(1_000), 900);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typography_defaults_leave_non_editor_surfaces_on_system_settings() {
        let preferences = Preferences::default();
        assert_eq!(preferences.ui_typography.font_size, 0);
        assert_eq!(preferences.grid_typography.font_size, 0);
        assert_eq!(preferences.output_typography.font_size, 0);
        assert_eq!(preferences.editor_font_size, 0);
    }

    #[test]
    fn old_editor_font_settings_deserialize_into_the_editor_surface() {
        let preferences: Preferences = serde_json::from_str(
            r#"{"default_page_size":1000,"confirm_destructive":true,"editor_font_size":16,"editor_font_family":"Iosevka"}"#,
        )
        .unwrap();
        assert_eq!(preferences.editor_font_size, 16);
        assert_eq!(preferences.editor_font_family, "Iosevka");
    }

    #[test]
    fn a_preferences_file_from_before_the_setting_keeps_the_preview_tab_on() {
        let prefs: Preferences =
            serde_json::from_str(r#"{"default_page_size":1000,"confirm_destructive":true,"editor_font_size":12}"#)
                .unwrap();
        assert!(prefs.open_tables_in_preview_tab);
    }
    #[test]
    fn theme_rows_and_keys_agree_and_unknown_keys_fall_back() {
        for (position, theme) in ThemePref::ALL.into_iter().enumerate() {
            assert_eq!(ThemePref::from_index(position as u32), theme);
            assert_eq!(ThemePref::from_key(theme.key()), theme);
        }
        assert_eq!(ThemePref::from_key("neon"), ThemePref::Default);
        assert_eq!(ThemePref::from_index(99), ThemePref::Default);
        let old: Preferences =
            serde_json::from_str(r#"{"default_page_size":1000,"confirm_destructive":true,"editor_font_size":12}"#)
                .unwrap();
        assert_eq!(old.theme, ThemePref::Default);
    }

    #[test]
    fn null_display_rows_and_keys_agree_and_unknown_keys_fall_back() {
        for (position, style) in NullDisplayPref::ALL.into_iter().enumerate() {
            assert_eq!(style.index() as usize, position);
            assert_eq!(NullDisplayPref::from_index(position as u32), style);
            assert_eq!(NullDisplayPref::from_key(style.key()), style);
        }
        assert_eq!(NullDisplayPref::from_key("nonsense"), NullDisplayPref::Standard);
    }

    #[test]
    fn date_format_rows_and_keys_agree_and_unknown_keys_fall_back() {
        for (position, format) in DateFormatPref::ALL.into_iter().enumerate() {
            assert_eq!(format.index() as usize, position);
            assert_eq!(DateFormatPref::from_index(position as u32), format);
            assert_eq!(DateFormatPref::from_key(format.key()), format);
        }
        assert_eq!(DateFormatPref::from_key("nonsense"), DateFormatPref::Iso);
        let old: Preferences =
            serde_json::from_str(r#"{"default_page_size":1000,"confirm_destructive":true,"editor_font_size":12}"#)
                .unwrap();
        assert_eq!(old.date_format, DateFormatPref::Iso);
    }
    #[test]
    fn an_old_preferences_file_without_a_color_scheme_loads_with_the_system_default() {
        let parsed: Preferences =
            serde_json::from_str(r#"{"default_page_size":50,"confirm_destructive":true,"editor_font_size":12}"#)
                .unwrap();
        assert_eq!(parsed.color_scheme, ColorSchemePref::System);
    }

    #[test]
    fn an_old_preferences_file_marks_spaces_tabs_and_line_breaks_by_default() {
        let parsed: Preferences =
            serde_json::from_str(r#"{"default_page_size":1000,"confirm_destructive":true,"editor_font_size":12}"#)
                .unwrap();
        assert!(parsed.show_invisible_characters);
    }
}
