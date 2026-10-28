use relm4::gtk::gdk;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Palette {
    pub background: gdk::RGBA,
    pub card: gdk::RGBA,
    pub header: gdk::RGBA,
    pub border: gdk::RGBA,
    pub text: gdk::RGBA,
    pub dim_text: gdk::RGBA,
    pub line: gdk::RGBA,
    pub accent: gdk::RGBA,
    pub highlight: gdk::RGBA,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ThemeColors {
    pub window_bg: gdk::RGBA,
    pub card_bg: gdk::RGBA,
    pub card_fg: gdk::RGBA,
    pub border: gdk::RGBA,
    pub accent: gdk::RGBA,
}

const DIMMED: f32 = 0.55;

fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> gdk::RGBA {
    gdk::RGBA::new(red, green, blue, alpha)
}

pub(crate) fn theme(dark: bool) -> ThemeColors {
    if dark {
        ThemeColors {
            window_bg: rgba(0.14, 0.14, 0.15, 1.0),
            card_bg: rgba(1.0, 1.0, 1.0, 0.08),
            card_fg: rgba(1.0, 1.0, 1.0, 1.0),
            border: rgba(1.0, 1.0, 1.0, 0.15),
            accent: rgba(0.47, 0.68, 0.93, 1.0),
        }
    } else {
        ThemeColors {
            window_bg: rgba(0.96, 0.96, 0.96, 1.0),
            card_bg: rgba(1.0, 1.0, 1.0, 1.0),
            card_fg: rgba(0.0, 0.0, 0.0, 0.8),
            border: rgba(0.0, 0.0, 0.0, 0.15),
            accent: rgba(0.21, 0.52, 0.89, 1.0),
        }
    }
}

impl Palette {
    pub(crate) fn from_theme(theme: ThemeColors) -> Self {
        let background = opaque(theme.window_bg);
        let card = over(theme.card_bg, background);
        Self {
            background,
            card,
            header: over(with_alpha(theme.card_fg, 0.06), card),
            border: theme.border,
            text: theme.card_fg,
            dim_text: with_alpha(theme.card_fg, theme.card_fg.alpha() * DIMMED),
            line: with_alpha(theme.card_fg, theme.card_fg.alpha() * DIMMED),
            accent: theme.accent,
            highlight: over(with_alpha(theme.accent, 0.16), card),
        }
    }
}

fn with_alpha(color: gdk::RGBA, alpha: f32) -> gdk::RGBA {
    gdk::RGBA::new(color.red(), color.green(), color.blue(), alpha.clamp(0.0, 1.0))
}

fn opaque(color: gdk::RGBA) -> gdk::RGBA {
    with_alpha(color, 1.0)
}

pub(crate) fn over(top: gdk::RGBA, bottom: gdk::RGBA) -> gdk::RGBA {
    let alpha = top.alpha().clamp(0.0, 1.0);
    let mix = |a: f32, b: f32| a * alpha + b * (1.0 - alpha);
    gdk::RGBA::new(
        mix(top.red(), bottom.red()),
        mix(top.green(), bottom.green()),
        mix(top.blue(), bottom.blue()),
        1.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_translucent_card_is_made_opaque_over_the_window() {
        let card = over(rgba(1.0, 1.0, 1.0, 0.5), rgba(0.0, 0.0, 0.0, 1.0));

        assert_eq!(card, rgba(0.5, 0.5, 0.5, 1.0));
    }

    #[test]
    fn every_fill_is_opaque_and_text_keeps_the_theme_colour() {
        let palette = Palette::from_theme(ThemeColors {
            window_bg: rgba(0.13, 0.13, 0.15, 1.0),
            card_bg: rgba(1.0, 1.0, 1.0, 0.08),
            card_fg: rgba(1.0, 1.0, 1.0, 1.0),
            border: rgba(1.0, 1.0, 1.0, 0.15),
            accent: rgba(0.47, 0.68, 0.93, 1.0),
        });

        for fill in [palette.background, palette.card, palette.header, palette.highlight] {
            assert_eq!(fill.alpha(), 1.0, "{fill:?}");
        }
        assert_eq!(palette.text, rgba(1.0, 1.0, 1.0, 1.0));
        assert!(
            palette.card.red() > palette.background.red(),
            "a card stands out from the window"
        );
        assert!((palette.dim_text.alpha() - DIMMED).abs() < f32::EPSILON);
    }
}
