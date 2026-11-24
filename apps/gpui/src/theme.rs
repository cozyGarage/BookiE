pub mod size {
    pub const HAIRLINE: f32 = 1.0;
    pub const OPTICAL: f32 = 2.0;
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const COMPACT_CONTROL: f32 = 24.0;
    pub const SMALL_CONTROL: f32 = 28.0;
    pub const GRID_HEADER: f32 = 32.0;
    pub const FORM_CONTROL: f32 = 32.0;
    pub const CONTROL_RADIUS: f32 = 4.0;
    pub const SURFACE_RADIUS: f32 = 6.0;
    pub const TEXT_SMALL: f32 = 12.0;
    pub const SAVE_PREVIEW_MAX_HEIGHT: f32 = 160.0;
    pub const STRUCTURE_PANEL_MAX_HEIGHT: f32 = 280.0;
    pub const DETAIL_SIDEBAR_WIDTH: f32 = 380.0;
    pub const STATUS_COMPACT_WIDTH: f32 = 1000.0;
    pub const WRAP_LINES: usize = 3;
    pub const WRAPPED_ROW_HEIGHT: f32 = 62.0;
    pub const STATUS_CONNECTION_WIDTH: f32 = 360.0;
    pub const STATUS_SQL_PREVIEW_WIDTH: f32 = 320.0;
    pub const DETAIL_SIDEBAR_MIN_WIDTH: f32 = 240.0;
    pub const CHART_COLUMN_WIDTH: f32 = 180.0;
    pub const GUTTER_MARKER_WIDTH: f32 = 3.0;
    pub const FULL_VALUE_EDITOR_WIDTH: f32 = 760.0;
    pub const APPROVAL_DIALOG_WIDTH: f32 = 560.0;
    pub const SHORTCUT_HINT_WIDTH: f32 = 320.0;
    pub const TOOLBAR_LABELS_MIN_WIDTH: f32 = 900.0;
    pub const SHORTCUT_CHORD_WIDTH: f32 = 120.0;
    pub const FULL_VALUE_EDITOR_HEIGHT: f32 = 560.0;
    pub const TEXT: f32 = 13.0;
    pub const STAT_TEXT: f32 = 22.0;
    pub const TREE_ROW: f32 = 26.0;
    pub const LIST_ROW: f32 = 36.0;
    pub const EDITOR_LINE: f32 = 20.0;
    pub const ICON: f32 = 14.0;
    pub const CHEVRON: f32 = ICON;
    pub const SPLITTER: f32 = 5.0;
    pub const MENU_WIDTH: f32 = 220.0;
    pub const SIDE_PANEL_WIDTH: f32 = 300.0;
    pub const FIND_WIDTH: f32 = 160.0;
    pub const FILTER_WIDTH: f32 = 420.0;
    pub const PALETTE_WIDTH: f32 = 520.0;
    pub const PALETTE_TOP: f32 = 90.0;
    pub const HOME_WIDTH: f32 = 980.0;
    pub const SETTINGS_SIDEBAR_WIDTH: f32 = 160.0;
    pub const HOME_TOP: f32 = 96.0;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub radius: f32,
    pub background: u32,
    pub foreground: u32,
    pub muted: u32,
    pub muted_foreground: u32,
    pub chrome: u32,
    pub sidebar: u32,
    pub popover: u32,
    pub border: u32,
    pub focus_ring: u32,
    pub accent: u32,
    pub stripe: u32,
    pub active_row: u32,
    pub info: u32,
    pub success: u32,
    pub warning: u32,
    pub destructive: u32,
    pub edited: u32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    pub fn light() -> Self {
        Theme {
            dark: false,
            radius: size::CONTROL_RADIUS,
            background: 0xffffff,
            foreground: 0x0a0a0a,
            muted: 0xf5f5f5,
            muted_foreground: 0x6b6b6b,
            chrome: 0xf5f5f5,
            sidebar: 0xfafafa,
            popover: 0xffffff,
            border: 0xe5e5e5,
            focus_ring: 0x8a8a8a,
            accent: 0xf5f5f5,
            stripe: 0xf0f0f0,
            active_row: 0xe5e5e5,
            info: 0x2563eb,
            success: 0x16a34a,
            warning: 0xb45309,
            destructive: 0xb91c1c,
            edited: 0xfef3c7,
        }
    }

    pub fn dark() -> Self {
        Theme {
            dark: true,
            radius: size::CONTROL_RADIUS,
            background: 0x131416,
            foreground: 0xd7d7db,
            muted: 0x2a2a2d,
            muted_foreground: 0x97989d,
            chrome: 0x1b1b1e,
            sidebar: 0x19191c,
            popover: 0x1e1e20,
            border: 0x3a3a3d,
            focus_ring: 0x85868b,
            accent: 0x2e2f33,
            stripe: 0x28282b,
            active_row: 0x3a3a3d,
            info: 0x60a5fa,
            success: 0x4ade80,
            warning: 0xfbbf24,
            destructive: 0xf3625f,
            edited: 0x4a3b12,
        }
    }

    pub fn named(name: Option<&str>) -> Self {
        match name {
            Some("dark" | "dbx-dark") => Self::dark(),
            _ => Self::light(),
        }
    }

    pub fn toggled(self) -> Self {
        if self.dark { Self::light() } else { Self::dark() }
    }

    pub fn label(self) -> &'static str {
        if self.dark { "Dark" } else { "Light" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(colour: u32) -> f64 {
        let channel = |shift: u32| {
            let value = f64::from((colour >> shift) & 0xff) / 255.0;
            if value <= 0.039_28 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    fn contrast(a: u32, b: u32) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_and_indicators_reach_their_contrast_on_every_surface_in_both_themes() {
        let mut failures = Vec::new();
        for theme in [Theme::light(), Theme::dark()] {
            let surfaces = [
                ("background", theme.background),
                ("chrome", theme.chrome),
                ("sidebar", theme.sidebar),
                ("popover", theme.popover),
                ("muted", theme.muted),
            ];
            let roles = [
                ("foreground", theme.foreground, 4.5),
                ("muted_foreground", theme.muted_foreground, 4.5),
                ("destructive", theme.destructive, 4.5),
                ("info", theme.info, 4.5),
                ("warning", theme.warning, 3.0),
                ("success", theme.success, 3.0),
                ("focus_ring", theme.focus_ring, 3.0),
            ];
            for (surface, background) in surfaces {
                for (role, colour, minimum) in roles {
                    let ratio = contrast(colour, background);
                    if ratio < minimum {
                        failures.push(format!("{} {role} on {surface}: {ratio:.2} < {minimum}", theme.label()));
                    }
                }
            }
            for (fill, colour) in [("active_row", theme.active_row), ("edited", theme.edited), ("stripe", theme.stripe)] {
                let ratio = contrast(theme.foreground, colour);
                if ratio < 4.5 {
                    failures.push(format!("{} foreground on {fill}: {ratio:.2} < 4.5", theme.label()));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn light_is_the_default_and_uses_the_design_tokens() {
        let theme = Theme::named(None);
        assert_eq!(theme, Theme::light());
        assert_eq!(
            (theme.background, theme.foreground, theme.border),
            (0xffffff, 0x0a0a0a, 0xe5e5e5)
        );
        assert_eq!(
            (theme.muted_foreground, theme.stripe, theme.active_row),
            (0x6b6b6b, 0xf0f0f0, 0xe5e5e5)
        );
        assert_eq!(theme.radius, size::CONTROL_RADIUS);
    }

    #[test]
    fn both_themes_define_the_semantic_colours_of_the_design() {
        for theme in [Theme::light(), Theme::dark()] {
            assert_ne!(theme.destructive, theme.success);
            assert_ne!(theme.focus_ring, theme.border);
            assert_ne!(theme.popover, 0);
        }
        assert_eq!(
            (Theme::light().destructive, Theme::dark().success),
            (0xb91c1c, 0x4ade80)
        );
    }

    #[test]
    fn a_name_picks_the_theme_and_an_unknown_name_falls_back_to_light() {
        assert_eq!(Theme::named(Some("dark")), Theme::dark());
        assert_eq!(Theme::named(Some("solarized")), Theme::light());
    }

    #[test]
    fn toggling_twice_returns_to_the_start() {
        assert_eq!(Theme::light().toggled(), Theme::dark());
        assert_eq!(Theme::light().toggled().toggled(), Theme::light());
        assert_eq!(Theme::dark().label(), "Dark");
    }
}
