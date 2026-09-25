use std::sync::{Arc, Mutex};

use gtk4::gio;
use gtk4::gio::prelude::*;
use serde::{Deserialize, Serialize};

use super::config_io::{atomic_write_json, backup_file_if_exists, xdg_config_path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    pub default_page_size: u64,
    pub confirm_destructive: bool,
    pub editor_font_size: u32,
    #[serde(default = "default_history_retention_days")]
    pub history_retention_days: u32,
    /// Wall-clock seconds before the editor's Run cancels a query
    /// the driver hasn't returned from. `0` disables the timeout.
    /// Defaults to 60s — long enough for typical OLTP work and
    /// catalog browsing, short enough that a runaway DDL or
    /// cross-join doesn't pin the GTK main thread waiting on
    /// shutdown.
    #[serde(default = "default_query_timeout_secs")]
    pub query_timeout_secs: u32,
    #[serde(default = "default_csv_include_header")]
    pub csv_include_header: bool,
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

impl Default for Preferences {
    fn default() -> Self {
        Self {
            default_page_size: 1_000,
            confirm_destructive: true,
            editor_font_size: 12,
            history_retention_days: default_history_retention_days(),
            query_timeout_secs: default_query_timeout_secs(),
            csv_include_header: default_csv_include_header(),
        }
    }
}

/// Mirrors the cache in `filter_settings.rs` / `column_widths.rs`. Without
/// it, every caller -- including `operation_control::configured_timeout_secs`,
/// read on the GTK thread before each query dispatch -- did a synchronous
/// file read on the main thread for a value that only ever changes from the
/// Preferences dialog. App-owned and shared explicitly rather than a global,
/// so tests and multiple windows never fight over one process-wide cache.
#[derive(Debug, Clone, Default)]
pub struct PreferencesStore(Arc<Mutex<Option<Preferences>>>);

impl PreferencesStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&self) -> Preferences {
        let mut guard = match self.0.lock() {
            Ok(g) => g,
            Err(_) => return load_from_disk(),
        };
        guard.get_or_insert_with(load_from_disk).clone()
    }

    pub fn save(&self, prefs: &Preferences) {
        let Some(path) = xdg_config_path("preferences.json") else {
            return;
        };
        let settings = settings();
        if let Some(ref settings) = settings
            && let Err(e) = backup_legacy(settings, &path)
        {
            tracing::warn!(path = %path.display(), error = %e, "preferences: backup failed");
            return;
        }
        if let Err(e) = atomic_write_json(&path, prefs) {
            tracing::warn!(path = %path.display(), error = %e, "preferences: write failed");
            return;
        }
        if let Some(settings) = settings
            && let Err(e) = save_settings(&settings, prefs)
        {
            tracing::warn!(error = %e, "preferences: GSettings write failed");
        }
        if let Ok(mut guard) = self.0.lock() {
            *guard = Some(prefs.clone());
        }
    }

    pub fn update(&self, mutate: impl FnOnce(&mut Preferences)) {
        let mut prefs = self.load();
        mutate(&mut prefs);
        self.save(&prefs);
    }
}

fn load_from_disk() -> Preferences {
    let Some(path) = xdg_config_path("preferences.json") else {
        return Preferences::default();
    };
    let legacy = std::fs::read(&path).ok();
    let Some(settings) = settings() else {
        return legacy
            .as_deref()
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or_default();
    };
    if settings.boolean("preferences-migrated") {
        let current = read_settings(&settings);
        if let Some(prefs) = legacy
            .as_deref()
            .and_then(|b| serde_json::from_slice::<Preferences>(b).ok())
            && prefs != current
        {
            if let Err(e) = save_settings(&settings, &prefs) {
                tracing::warn!(error = %e, "preferences: rollback changes not synced to GSettings");
            }
            return prefs;
        }
        return current;
    }
    let prefs = match legacy.as_deref() {
        Some(bytes) => match serde_json::from_slice(bytes) {
            Ok(prefs) => prefs,
            Err(e) => {
                tracing::warn!(error = %e, "preferences: invalid legacy JSON; migration deferred");
                return Preferences::default();
            }
        },
        None => Preferences::default(),
    };
    if let Err(e) = backup_legacy(&settings, &path).and_then(|()| save_settings(&settings, &prefs)) {
        tracing::warn!(error = %e, "preferences: GSettings migration deferred");
    }
    prefs
}

pub(super) fn settings() -> Option<gio::Settings> {
    gio::SettingsSchemaSource::default()?
        .lookup(crate::config::APP_ID, true)
        .map(|schema| gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None))
}

fn read_settings(settings: &gio::Settings) -> Preferences {
    Preferences {
        default_page_size: settings.uint64("default-page-size"),
        confirm_destructive: settings.boolean("confirm-destructive"),
        editor_font_size: settings.uint("editor-font-size"),
        history_retention_days: settings.uint("history-retention-days"),
        query_timeout_secs: settings.uint("query-timeout-secs"),
        csv_include_header: settings.boolean("csv-include-header"),
    }
}

fn backup_legacy(settings: &gio::Settings, path: &std::path::Path) -> Result<(), String> {
    if !settings.boolean("preferences-migrated") {
        backup_file_if_exists(path, &path.with_extension("before-gsettings.json")).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn save_settings(settings: &gio::Settings, prefs: &Preferences) -> Result<(), String> {
    settings.delay();
    let result = (|| {
        settings.set_uint64("default-page-size", prefs.default_page_size)?;
        settings.set_boolean("confirm-destructive", prefs.confirm_destructive)?;
        settings.set_uint("editor-font-size", prefs.editor_font_size)?;
        settings.set_uint("history-retention-days", prefs.history_retention_days)?;
        settings.set_uint("query-timeout-secs", prefs.query_timeout_secs)?;
        settings.set_boolean("csv-include-header", prefs.csv_include_header)?;
        settings.set_boolean("preferences-migrated", true)?;
        Ok::<(), glib::BoolError>(())
    })();
    if let Err(e) = result {
        settings.revert();
        return Err(e.to_string());
    }
    settings.apply();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Seeds the cache directly rather than calling `save`, so the test
    /// never touches the real XDG config file: `load` must return exactly
    /// what's cached instead of re-reading (or falling back to a default
    /// because it can't read) disk.
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
            let backup = path.with_extension("before-gsettings.json");
            assert_eq!(std::fs::read(&backup).unwrap(), std::fs::read(&path).unwrap());
            assert_eq!(std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777, 0o600);
            assert!(settings().unwrap().boolean("preferences-migrated"));
            store.update(|prefs| prefs.default_page_size = 654);
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
            super::super::window_state::save_geometry(902, 703, true);
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
        std::fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/com.tablepro.linux.gschema.xml"),
            &schema,
        )
        .unwrap();
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

    #[test]
    fn load_reads_the_cache_instead_of_disk_once_populated() {
        let sentinel = Preferences {
            default_page_size: 424_242,
            ..Preferences::default()
        };
        let store = PreferencesStore::new();
        *store.0.lock().unwrap() = Some(sentinel.clone());
        assert_eq!(store.load().default_page_size, sentinel.default_page_size);
    }
}
