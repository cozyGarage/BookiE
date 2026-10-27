use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn runtime_schema_directory_is_used_for_settings() {
    if std::env::var_os("BOOKIE_RUNTIME_SCHEMA_TEST").is_some() {
        assert_eq!(open_settings().unwrap().uint64("default-page-size"), 777);
        assert_eq!(
            make_test_settings("/com/tablepro/linux/runtime/")
                .0
                .uint64("default-page-size"),
            777
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let xml = include_str!("../../../data/com.tablepro.linux.gschema.xml")
        .replace("<default>1000</default>", "<default>777</default>");
    std::fs::write(directory.path().join("com.tablepro.linux.gschema.xml"), xml).unwrap();
    assert!(
        std::process::Command::new("glib-compile-schemas")
            .arg("--strict")
            .arg(directory.path())
            .status()
            .unwrap()
            .success()
    );
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("services::preferences::tests::runtime_schema_directory_is_used_for_settings")
        .env("BOOKIE_RUNTIME_SCHEMA_TEST", "1")
        .env("GSETTINGS_SCHEMA_DIR", directory.path())
        .env("GSETTINGS_BACKEND", "memory")
        .env("XDG_CONFIG_HOME", directory.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn migrates_legacy_settings_and_keeps_rollback_copies() {
    if std::env::var_os("BOOKIE_SETTINGS_TEST_CHILD").is_some() {
        let path = xdg_config_path("preferences.json").unwrap();
        atomic_write_json(
            &path,
            &Preferences {
                default_page_size: 321,
                ..Preferences::default()
            },
        )
        .unwrap();
        let store = PreferencesStore::new();
        assert_eq!(store.load().default_page_size, 321);
        let backup = backup_path(&path).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), std::fs::read(&path).unwrap());
        assert_eq!(std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(
            settings().unwrap().uint("migration-version"),
            SETTINGS_MIGRATION_VERSION
        );
        store.update(|prefs| prefs.default_page_size = 654).unwrap();
        assert_eq!(PreferencesStore::new().load().default_page_size, 654);
        assert_eq!(
            serde_json::from_slice::<Preferences>(&std::fs::read(&path).unwrap())
                .unwrap()
                .default_page_size,
            654
        );
        assert_eq!(
            serde_json::from_slice::<Preferences>(&std::fs::read(&backup).unwrap())
                .unwrap()
                .default_page_size,
            321
        );
        atomic_write_json(
            &path,
            &Preferences {
                default_page_size: 777,
                ..Preferences::default()
            },
        )
        .unwrap();
        assert_eq!(PreferencesStore::new().load().default_page_size, 777);
        assert_eq!(settings().unwrap().uint64("default-page-size"), 777);
        let window_path = xdg_config_path("window.json").unwrap();
        let connection_id = uuid::Uuid::new_v4();
        let old_window = super::super::window_state::WindowState {
            width: 901,
            height: 702,
            maximized: false,
            last_connection_id: Some(connection_id),
        };
        atomic_write_json(&window_path, &old_window).unwrap();
        assert_eq!(super::super::window_state::load().width, 901);
        assert_eq!(
            std::fs::read(window_path.with_extension("before-gsettings.json")).unwrap(),
            std::fs::read(&window_path).unwrap()
        );
        let updated_state = super::super::window_state::WindowState {
            width: 902,
            height: 703,
            maximized: true,
            last_connection_id: Some(connection_id),
        };
        let generation = super::super::window_state::next_save_generation();
        super::super::window_state::save(&updated_state, generation).unwrap();
        let updated = super::super::window_state::load();
        assert_eq!(updated.last_connection_id, Some(connection_id));
        assert_eq!(updated.width, 902);
        assert_eq!(settings().unwrap().int("window-width"), 902);
        atomic_write_json(&window_path, &old_window).unwrap();
        assert_eq!(super::super::window_state::load().width, 901);
        assert_eq!(settings().unwrap().int("window-width"), 901);
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let schema = dir.path().join("com.tablepro.linux.gschema.xml");
    std::fs::write(&schema, include_str!("../../../data/com.tablepro.linux.gschema.xml")).unwrap();
    assert!(
        std::process::Command::new("glib-compile-schemas")
            .arg("--strict")
            .arg(dir.path())
            .status()
            .unwrap()
            .success()
    );
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("migrates_legacy_settings_and_keeps_rollback_copies")
        .env("BOOKIE_SETTINGS_TEST_CHILD", "1")
        .env("GSETTINGS_SCHEMA_DIR", dir.path())
        .env("GSETTINGS_BACKEND", "memory")
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stdout));
}

fn make_test_settings(_path: &str) -> (gio::Settings, gio::SettingsBackend) {
    let source = gio::SettingsSchemaSource::from_directory(
        schema_directory(),
        gio::SettingsSchemaSource::default().as_ref(),
        false,
    )
    .unwrap();
    let schema = source.lookup(SETTINGS_SCHEMA, true).unwrap();
    let backend = gio::memory_settings_backend_new();
    let settings = gio::Settings::new_full(&schema, Some(&backend), None);
    (settings, backend)
}

#[test]
fn gsettings_updates_are_visible_to_reopened_handles() {
    let (settings, backend) = make_test_settings("/com/tablepro/linux/test/");
    let store = PreferencesStore::from_settings(settings, None);
    store
        .update(|prefs| {
            prefs.default_page_size = 424_242;
            prefs.csv_include_header = false;
        })
        .unwrap();
    store.flush().unwrap();

    let reopened = test_settings_with_backend("/com/tablepro/linux/test/", backend);
    let reopened = PreferencesStore::from_settings(reopened, None);
    assert_eq!(reopened.load().default_page_size, 424_242);
    assert!(!reopened.load().csv_include_header);
}

#[test]
fn production_and_development_profiles_use_separate_gsettings_paths() {
    let production = settings_path_for_profile(crate::config::Profile::Default);
    let development = settings_path_for_profile(crate::config::Profile::Development);
    assert_ne!(production, development);
    assert!(production.starts_with("/com/tablepro/linux/"));
    assert!(development.starts_with("/com/tablepro/linux/Devel/"));
}

#[test]
fn gsettings_keyfile_backend_survives_a_process_restart() {
    let config = tempfile::tempdir().unwrap();
    for mode in ["write", "read"] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "services::preferences::tests::gsettings_keyfile_child",
                "--nocapture",
            ])
            .env("TABLEPRO_GSETTINGS_CHILD", mode)
            .env("GSETTINGS_BACKEND", "keyfile")
            .env("XDG_CONFIG_HOME", config.path())
            .status()
            .unwrap();
        assert!(status.success(), "GSettings {mode} child failed");
    }
}

#[test]
fn gsettings_keyfile_child() {
    let Ok(mode) = std::env::var("TABLEPRO_GSETTINGS_CHILD") else {
        return;
    };
    assert!(matches!(mode.as_str(), "write" | "read"));
    let store = PreferencesStore::new();
    match mode.as_str() {
        "write" => {
            store
                .update(|prefs| {
                    prefs.default_page_size = 313_131;
                    prefs.csv_include_header = false;
                })
                .unwrap();
            store.flush().unwrap();
        }
        "read" => {
            let prefs = store.load();
            assert_eq!(prefs.default_page_size, 313_131);
            assert!(!prefs.csv_include_header);
        }
        _ => {}
    }
}

#[test]
fn legacy_preferences_migrate_once_and_source_file_is_a_rollback_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let original = serde_json::to_vec(&Preferences {
        default_page_size: 777,
        editor_font_size: 15,
        csv_include_header: false,
        ..Preferences::default()
    })
    .unwrap();
    std::fs::write(&path, &original).unwrap();
    let (settings, backend) = make_test_settings("/com/tablepro/linux/migration/");

    migrate_json_preferences(&settings, Some(&path)).unwrap();

    let store = PreferencesStore::from_settings(settings.clone(), Some(path.clone()));
    assert_eq!(store.load().default_page_size, 777);
    assert_eq!(store.load().editor_font_size, 15);
    assert!(!store.load().csv_include_header);
    assert_eq!(settings.uint("migration-version"), SETTINGS_MIGRATION_VERSION);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(backup_path(&path).unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    store.update(|prefs| prefs.default_page_size = 999).unwrap();
    store.flush().unwrap();
    let downgraded_preferences: Preferences = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(downgraded_preferences.default_page_size, 999);
    assert_eq!(std::fs::read(backup_path(&path).unwrap()).unwrap(), original);

    let reopened = test_settings_with_backend("/com/tablepro/linux/migration/", backend);
    migrate_json_preferences(&reopened, Some(&path)).unwrap();
    assert_eq!(read_settings(&reopened).default_page_size, 999);
}

#[test]
fn remote_migration_marker_keeps_settings_and_imports_rollback_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let (settings, _) = make_test_settings("/com/tablepro/linux/remote/");
    settings.set_boolean("preferences-migrated", true).unwrap();
    settings.set_uint64("default-page-size", 555).unwrap();
    migrate_json_preferences(&settings, Some(&path)).unwrap();
    assert_eq!(read_settings(&settings).default_page_size, 555);
    let legacy = Preferences {
        default_page_size: 777,
        ..Preferences::default()
    };
    atomic_write_json(&path, &legacy).unwrap();
    migrate_json_preferences(&settings, Some(&path)).unwrap();
    assert_eq!(read_settings(&settings), legacy);
    std::fs::write(&path, b"broken json").unwrap();
    let store = PreferencesStore::from_settings_with_legacy(settings, Some(path.clone()));
    assert!(store.update(|prefs| prefs.default_page_size = 1).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"broken json");
}

#[test]
fn malformed_legacy_preferences_are_preserved_and_migration_can_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    std::fs::write(&path, b"broken json").unwrap();
    let (settings, _) = make_test_settings("/com/tablepro/linux/bad-migration/");

    assert!(migrate_json_preferences(&settings, Some(&path)).is_err());
    assert_eq!(settings.uint("migration-version"), 0);
    assert_eq!(std::fs::read(&path).unwrap(), b"broken json");
    let store = PreferencesStore::from_settings_with_legacy(settings.clone(), Some(path.clone()));
    assert!(store.update(|prefs| prefs.default_page_size = 500).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken json");
    std::fs::write(&path, serde_json::to_vec(&Preferences::default()).unwrap()).unwrap();
    migrate_json_preferences(&settings, Some(&path)).unwrap();
    assert_eq!(settings.uint("migration-version"), SETTINGS_MIGRATION_VERSION);
}

#[test]
fn a_newer_gsettings_migration_marker_is_not_downgraded() {
    let (settings, _) = make_test_settings("/com/tablepro/linux/future-version/");
    settings
        .set_uint("migration-version", SETTINGS_MIGRATION_VERSION + 1)
        .unwrap();

    assert!(migrate_json_preferences(&settings, None).is_err());
    assert_eq!(settings.uint("migration-version"), SETTINGS_MIGRATION_VERSION + 1);
}

#[test]
fn failed_legacy_mirror_rolls_back_the_gsettings_update() {
    let dir = tempfile::tempdir().unwrap();
    let parent_file = dir.path().join("not-a-directory");
    std::fs::write(&parent_file, b"block directory creation").unwrap();
    let path = parent_file.join("preferences.json");
    let (settings, _) = make_test_settings("/com/tablepro/linux/mirror-failure/");
    let store = PreferencesStore::from_settings(settings, Some(path));

    assert!(store.update(|prefs| prefs.default_page_size = 500).is_err());

    assert_eq!(store.load().default_page_size, Preferences::default().default_page_size);
}

fn test_settings_with_backend(_path: &str, backend: gio::SettingsBackend) -> gio::Settings {
    let source = gio::SettingsSchemaSource::from_directory(
        schema_directory(),
        gio::SettingsSchemaSource::default().as_ref(),
        false,
    )
    .unwrap();
    let schema = source.lookup(SETTINGS_SCHEMA, true).unwrap();
    gio::Settings::new_full(&schema, Some(&backend), None)
}

fn after_a_round_trip(path: &str, change: impl FnOnce(&mut Preferences)) -> (Preferences, Preferences) {
    let (settings, backend) = make_test_settings(path);
    let store = PreferencesStore::from_settings(settings, None);
    let before = store.load();
    store.update(change).unwrap();
    store.flush().unwrap();
    let reopened = PreferencesStore::from_settings(test_settings_with_backend(path, backend), None);
    (before, reopened.load())
}

#[test]
fn the_color_scheme_survives_a_gsettings_round_trip_and_defaults_to_the_system() {
    let (before, after) = after_a_round_trip("/com/tablepro/linux/scheme/", |prefs| {
        prefs.color_scheme = ColorSchemePref::Dark;
    });
    assert_eq!(before.color_scheme, ColorSchemePref::System);
    assert_eq!(after.color_scheme, ColorSchemePref::Dark);
}

#[test]
fn the_editor_font_family_survives_a_gsettings_round_trip_and_defaults_to_empty() {
    let (before, after) = after_a_round_trip("/com/tablepro/linux/font-family/", |prefs| {
        prefs.editor_font_family = "Iosevka".into();
    });
    assert_eq!(before.editor_font_family, "");
    assert_eq!(after.editor_font_family, "Iosevka");
}

#[test]
fn the_theme_defaults_to_default_and_survives_a_gsettings_round_trip() {
    let (before, after) = after_a_round_trip("/com/tablepro/linux/theme/", |prefs| {
        prefs.theme = ThemePref::Gruvbox;
    });
    assert_eq!(before.theme, ThemePref::Default);
    assert_eq!(after.theme, ThemePref::Gruvbox);
}

#[test]
fn vim_mode_is_off_by_default_and_survives_a_gsettings_round_trip() {
    let (before, after) = after_a_round_trip("/com/tablepro/linux/vim-mode/", |prefs| {
        prefs.editor_vim_mode = true;
    });
    assert!(!before.editor_vim_mode);
    assert!(after.editor_vim_mode);
}

#[test]
fn opening_a_table_reuses_the_preview_tab_by_default_and_the_choice_survives_a_gsettings_round_trip() {
    let (before, after) = after_a_round_trip("/com/tablepro/linux/preview-tab/", |prefs| {
        prefs.open_tables_in_preview_tab = false;
    });
    assert!(before.open_tables_in_preview_tab);
    assert!(!after.open_tables_in_preview_tab);
}

#[test]
fn the_null_display_survives_a_gsettings_round_trip_and_defaults_to_standard() {
    let (before, after) = after_a_round_trip("/com/tablepro/linux/null-display/", |prefs| {
        prefs.null_display = NullDisplayPref::Symbol;
    });
    assert_eq!(before.null_display, NullDisplayPref::Standard);
    assert_eq!(after.null_display, NullDisplayPref::Symbol);
}

#[test]
fn color_scheme_rows_keys_and_styles_agree() {
    for (position, scheme) in ColorSchemePref::ALL.into_iter().enumerate() {
        assert_eq!(scheme.index() as usize, position);
        assert_eq!(ColorSchemePref::from_index(position as u32), scheme);
        assert_eq!(ColorSchemePref::from_key(scheme.key()), scheme);
    }
    assert_eq!(ColorSchemePref::from_key("sepia"), ColorSchemePref::System);
    assert_eq!(ColorSchemePref::from_index(9), ColorSchemePref::System);
    assert_eq!(color_scheme_style(ColorSchemePref::Light), adw::ColorScheme::ForceLight);
    assert_eq!(color_scheme_style(ColorSchemePref::Dark), adw::ColorScheme::ForceDark);
    assert_eq!(color_scheme_style(ColorSchemePref::System), adw::ColorScheme::Default);
}
