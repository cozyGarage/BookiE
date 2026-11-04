use gpui::{App, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub dark: bool,
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

impl Global for Theme {}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    pub fn light() -> Self {
        Theme {
            dark: false,
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

    pub fn dark() -> Self {
        Theme {
            dark: true,
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

    pub fn named(name: Option<&str>) -> Self {
        match name {
            Some("dark") => Self::dark(),
            _ => Self::light(),
        }
    }

    pub fn toggled(self) -> Self {
        if self.dark { Self::light() } else { Self::dark() }
    }

    pub fn current(cx: &App) -> Self {
        cx.try_global::<Theme>().copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_is_the_default_and_uses_dbx_tokens() {
        let theme = Theme::named(None);
        assert_eq!(theme, Theme::light());
        assert_eq!(
            (theme.background, theme.foreground, theme.border),
            (0xffffff, 0x0a0a0a, 0xe5e5e5)
        );
        assert_eq!(
            (theme.muted_foreground, theme.stripe, theme.active_row),
            (0x737373, 0xf0f0f0, 0xdbeafe)
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
    }
}
