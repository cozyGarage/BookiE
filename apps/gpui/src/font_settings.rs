use std::path::PathBuf;

pub const MIN_SIZE: f32 = 6.0;
pub const MAX_SIZE: f32 = 100.0;
const DEFAULT_SIZE: f32 = 14.0;

#[derive(Debug, Clone, PartialEq)]
pub struct FontSettings {
    pub ui_family: String,
    pub ui_size: f32,
    pub editor_family: String,
    pub editor_size: f32,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            ui_family: String::new(),
            ui_size: DEFAULT_SIZE,
            editor_family: String::new(),
            editor_size: DEFAULT_SIZE,
        }
    }
}

fn size(value: Option<&serde_json::Value>) -> f32 {
    value
        .and_then(serde_json::Value::as_f64)
        .map_or(DEFAULT_SIZE, |size| (size as f32).clamp(MIN_SIZE, MAX_SIZE))
}

fn family(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn parse(text: &str) -> FontSettings {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return FontSettings::default();
    };
    FontSettings {
        ui_family: family(value.get("ui_font_family")),
        ui_size: size(value.get("ui_font_size")),
        editor_family: family(value.get("buffer_font_family")),
        editor_size: size(value.get("buffer_font_size")),
    }
}

pub fn to_json(settings: &FontSettings) -> String {
    serde_json::json!({
        "ui_font_family": settings.ui_family,
        "ui_font_size": settings.ui_size,
        "buffer_font_family": settings.editor_family,
        "buffer_font_size": settings.editor_size,
    })
    .to_string()
}

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("bookie-gpui").join("settings.json"))
}

pub fn load() -> FontSettings {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map_or_else(FontSettings::default, |text| parse(&text))
}

pub fn save(settings: &FontSettings) -> std::io::Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, to_json(settings))
}

pub fn clamp_size(size: f64) -> f32 {
    (size as f32).clamp(MIN_SIZE, MAX_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_broken_file_gives_the_defaults() {
        assert_eq!(parse(""), FontSettings::default());
        assert_eq!(parse("{not json"), FontSettings::default());
    }

    #[test]
    fn sizes_are_clamped_to_the_zed_range() {
        let settings = parse(r#"{"ui_font_size": 2, "buffer_font_size": 400}"#);
        assert_eq!(settings.ui_size, MIN_SIZE);
        assert_eq!(settings.editor_size, MAX_SIZE);
    }

    #[test]
    fn a_typed_size_is_clamped_before_it_is_stored() {
        assert_eq!(clamp_size(0.0), MIN_SIZE);
        assert_eq!(clamp_size(1000.0), MAX_SIZE);
        assert_eq!(clamp_size(15.0), 15.0);
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let settings = FontSettings {
            ui_family: "Fira Sans".into(),
            ui_size: 15.0,
            editor_family: "Fira Code".into(),
            editor_size: 13.5,
        };
        assert_eq!(parse(&to_json(&settings)), settings);
    }
}
