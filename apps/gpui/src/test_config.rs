use std::sync::Once;

static ISOLATE: Once = Once::new();

pub(crate) fn isolate() {
    ISOLATE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("bookie-gpui-test-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary config directory");
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &dir);
            std::env::set_var("XDG_DATA_HOME", &dir);
        }
    });
}

#[test]
fn tests_save_settings_and_stores_under_a_temporary_directory() {
    isolate();
    let settings = bookie_gpui::font_settings::path().expect("a settings path");
    assert!(settings.starts_with(std::env::temp_dir()), "{}", settings.display());
    let widths = bookie_app_services::column_widths::ColumnWidthStore::open();
    assert!(widths.is_some());
}
