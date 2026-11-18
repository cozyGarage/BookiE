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
    pub const FORM_CONTROL: f32 = 32.0;
    pub const CONTROL_RADIUS: f32 = 4.0;
    pub const SURFACE_RADIUS: f32 = 6.0;
    pub const TEXT_SMALL: f32 = 12.0;
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
    pub const FIND_WIDTH: f32 = 110.0;
    pub const FILTER_WIDTH: f32 = 420.0;
    pub const PALETTE_WIDTH: f32 = 520.0;
    pub const PALETTE_TOP: f32 = 90.0;
    pub const HOME_WIDTH: f32 = 980.0;
    pub const HOME_TOP: f32 = 96.0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Dbx,
    Adwaita,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub style: Style,
    pub dark: bool,
    pub radius: f32,
    pub background: u32,
    pub foreground: u32,
    pub muted: u32,
    pub muted_foreground: u32,
    pub chrome: u32,
    pub border: u32,
    pub accent: u32,
    pub stripe: u32,
    pub active_row: u32,
    pub info: u32,
    pub warning: u32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    pub fn light() -> Self {
        Self::dbx_light()
    }

    pub fn dark() -> Self {
        Self::dbx_dark()
    }

    pub fn dbx_light() -> Self {
        Theme {
            style: Style::Dbx,
            dark: false,
            radius: 4.0,
            background: 0xffffff,
            foreground: 0x0a0a0a,
            muted: 0xf5f5f5,
            muted_foreground: 0x737373,
            chrome: 0xfafafa,
            border: 0xe5e5e5,
            accent: 0xf5f5f5,
            stripe: 0xf0f0f0,
            active_row: 0xdbeafe,
            info: 0x2563eb,
            warning: 0xd97706,
        }
    }

    pub fn dbx_dark() -> Self {
        Theme {
            style: Style::Dbx,
            dark: true,
            radius: 4.0,
            background: 0x131416,
            foreground: 0xd7d7db,
            muted: 0x2a2a2d,
            muted_foreground: 0x97989d,
            chrome: 0x19191c,
            border: 0x3a3a3d,
            accent: 0x2e2f33,
            stripe: 0x28282b,
            active_row: 0x1b2e46,
            info: 0x60a5fa,
            warning: 0xfbbf24,
        }
    }

    pub fn adwaita_light() -> Self {
        Theme {
            style: Style::Adwaita,
            dark: false,
            radius: 6.0,
            background: 0xffffff,
            foreground: 0x2e3436,
            muted: 0xf6f5f4,
            muted_foreground: 0x77767b,
            chrome: 0xebebed,
            border: 0xd8d8db,
            accent: 0xe4e4e7,
            stripe: 0xf9f9fa,
            active_row: 0xd9e7fb,
            info: 0x3584e4,
            warning: 0xe5a50a,
        }
    }

    pub fn adwaita_dark() -> Self {
        Theme {
            style: Style::Adwaita,
            dark: true,
            radius: 6.0,
            background: 0x1e1e1e,
            foreground: 0xf0f0f0,
            muted: 0x2b2b2b,
            muted_foreground: 0x9a9996,
            chrome: 0x2e2e32,
            border: 0x3d3d3d,
            accent: 0x3a3a3f,
            stripe: 0x242424,
            active_row: 0x1f3a5c,
            info: 0x78aeed,
            warning: 0xf8e45c,
        }
    }

    pub fn named(name: Option<&str>) -> Self {
        match name {
            Some("dark" | "dbx-dark") => Self::dbx_dark(),
            Some("adwaita-light") => Self::adwaita_light(),
            Some("adwaita-dark") => Self::adwaita_dark(),
            _ => Self::dbx_light(),
        }
    }

    pub fn with_style(self, style: Style) -> Self {
        match (style, self.dark) {
            (Style::Dbx, false) => Self::dbx_light(),
            (Style::Dbx, true) => Self::dbx_dark(),
            (Style::Adwaita, false) => Self::adwaita_light(),
            (Style::Adwaita, true) => Self::adwaita_dark(),
        }
    }

    pub fn toggled(self) -> Self {
        match (self.style, self.dark) {
            (Style::Dbx, false) => Self::dbx_dark(),
            (Style::Dbx, true) => Self::dbx_light(),
            (Style::Adwaita, false) => Self::adwaita_dark(),
            (Style::Adwaita, true) => Self::adwaita_light(),
        }
    }

    pub fn label(self) -> &'static str {
        match (self.style, self.dark) {
            (Style::Dbx, false) => "DBX light",
            (Style::Dbx, true) => "DBX dark",
            (Style::Adwaita, false) => "Adwaita light",
            (Style::Adwaita, true) => "Adwaita dark",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbx_light_is_the_default_and_uses_dbx_tokens() {
        let theme = Theme::named(None);
        assert_eq!(theme, Theme::dbx_light());
        assert_eq!(
            (theme.background, theme.foreground, theme.border),
            (0xffffff, 0x0a0a0a, 0xe5e5e5)
        );
        assert_eq!(
            (theme.muted_foreground, theme.stripe, theme.active_row),
            (0x737373, 0xf0f0f0, 0xdbeafe)
        );
        assert_eq!(theme.radius, 4.0);
    }

    #[test]
    fn adwaita_uses_the_gnome_accent_and_rounder_corners() {
        let theme = Theme::adwaita_light();
        assert_eq!((theme.info, theme.radius), (0x3584e4, 6.0));
        assert_eq!(Theme::adwaita_dark().chrome, 0x2e2e32);
    }

    #[test]
    fn a_name_picks_the_theme_and_an_unknown_name_falls_back_to_dbx_light() {
        assert_eq!(Theme::named(Some("dark")), Theme::dbx_dark());
        assert_eq!(Theme::named(Some("adwaita-dark")), Theme::adwaita_dark());
        assert_eq!(Theme::named(Some("adwaita-light")), Theme::adwaita_light());
        assert_eq!(Theme::named(Some("solarized")), Theme::dbx_light());
    }

    #[test]
    fn toggling_keeps_the_style_and_twice_returns_to_the_start() {
        assert_eq!(Theme::adwaita_light().toggled(), Theme::adwaita_dark());
        assert_eq!(Theme::dbx_light().toggled().toggled(), Theme::dbx_light());
    }

    #[test]
    fn switching_style_keeps_light_or_dark() {
        assert_eq!(Theme::dbx_dark().with_style(Style::Adwaita), Theme::adwaita_dark());
        assert_eq!(Theme::adwaita_light().with_style(Style::Dbx), Theme::dbx_light());
        assert_eq!(Theme::dbx_dark().label(), "DBX dark");
    }
}
