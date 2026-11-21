#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
}

impl WindowGeometry {
    pub fn visible_on(self, x: f32, y: f32, width: f32, height: f32) -> bool {
        let overlap_width = (self.x + self.width).min(x + width) - self.x.max(x);
        let overlap_height = (self.y + self.height).min(y + height) - self.y.max(y);
        overlap_width >= 64.0 && overlap_height >= 32.0
    }

    pub fn path() -> Option<std::path::PathBuf> {
        super::font_settings::path()?
            .parent()
            .map(|dir| dir.join("window.json"))
    }

    pub fn load() -> Option<Self> {
        let text = std::fs::read_to_string(Self::path()?).ok()?;
        Self::parse(&text)
    }

    pub fn save(self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        self.save_to(&path)
    }

    fn save_to(self, path: &std::path::Path) -> std::io::Result<()> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent)?;
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = parent.join(format!(".window-{}-{sequence}.tmp", std::process::id()));
        let contents =
            serde_json::json!({"width": self.width, "height": self.height, "x": self.x, "y": self.y}).to_string();
        let result = std::fs::write(&temporary, contents).and_then(|()| std::fs::rename(&temporary, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    pub fn parse(text: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let geometry = Self {
            width: value.get("width")?.as_f64()? as f32,
            height: value.get("height")?.as_f64()? as f32,
            x: value.get("x")?.as_f64()? as f32,
            y: value.get("y")?.as_f64()? as f32,
        };
        ((400.0..=16384.0).contains(&geometry.width)
            && (300.0..=16384.0).contains(&geometry.height)
            && (-32768.0..=32768.0).contains(&geometry.x)
            && (-32768.0..=32768.0).contains(&geometry.y))
        .then_some(geometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_geometry_is_rejected() {
        assert!(WindowGeometry::parse("not json").is_none());
        assert!(WindowGeometry::parse(r#"{"width":0,"height":800,"x":0,"y":0}"#).is_none());
        assert!(WindowGeometry::parse(r#"{"width":1100,"height":800,"x":999999,"y":0}"#).is_none());
        assert_eq!(
            WindowGeometry::parse(r#"{"width":1100,"height":800,"x":10,"y":20}"#),
            Some(WindowGeometry {
                width: 1100.0,
                height: 800.0,
                x: 10.0,
                y: 20.0,
            })
        );
    }

    #[test]
    fn a_missing_display_or_tiny_overlap_does_not_restore_an_unreachable_window() {
        let geometry = WindowGeometry {
            width: 1100.0,
            height: 800.0,
            x: 1920.0,
            y: 0.0,
        };
        assert!(!geometry.visible_on(0.0, 0.0, 1920.0, 1080.0));
        assert!(!geometry.visible_on(0.0, 0.0, 1930.0, 1080.0));
        assert!(geometry.visible_on(1920.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn saving_twice_replaces_the_previous_geometry() -> std::io::Result<()> {
        let dir = std::env::temp_dir().join(format!("bookie-window-test-{}", std::process::id()));
        let path = dir.join("window.json");
        let mut geometry = WindowGeometry {
            width: 1100.0,
            height: 800.0,
            x: 10.0,
            y: 20.0,
        };
        geometry.save_to(&path)?;
        geometry.width = 1400.0;
        geometry.save_to(&path)?;
        assert_eq!(WindowGeometry::parse(&std::fs::read_to_string(&path)?), Some(geometry));
        std::fs::remove_dir_all(dir)
    }
}
