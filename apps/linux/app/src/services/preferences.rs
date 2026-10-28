use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use relm4::adw;

use super::config_io::{atomic_write_bytes, atomic_write_json, xdg_config_path};
use super::state_file::StateFile;

pub use tablepro_app_services::preferences::{ColorSchemePref, NullDisplayPref, Preferences, ThemePref};

pub fn color_scheme_style(scheme: ColorSchemePref) -> adw::ColorScheme {
    match scheme {
        ColorSchemePref::System => adw::ColorScheme::Default,
        ColorSchemePref::Light => adw::ColorScheme::ForceLight,
        ColorSchemePref::Dark => adw::ColorScheme::ForceDark,
    }
}

#[cfg(test)]
const SETTINGS_SCHEMA: &str = "com.tablepro.linux";
const SETTINGS_MIGRATION_VERSION: u32 = 1;

/// Preferences are shared explicitly rather than through a global. GSettings
/// is canonical; the legacy JSON file remains in place as a rollback copy.
/// JSON is retained as a fallback if the installed schema is unavailable.
#[derive(Clone)]
pub struct PreferencesStore(Backend);

#[derive(Clone)]
enum Backend {
    GSettings {
        settings: Rc<gio::Settings>,
        legacy_path: Option<PathBuf>,
        migration_error: Option<String>,
    },
    Json(Arc<StateFile<Preferences>>),
}

impl Default for PreferencesStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PreferencesStore {
    pub fn new() -> Self {
        let legacy_path = xdg_config_path("preferences.json");
        match open_settings() {
            Ok(settings) => Self::from_settings_with_legacy(settings, legacy_path),
            Err(error) => {
                tracing::warn!(%error, "preferences: GSettings unavailable; using JSON store");
                Self(Backend::Json(Arc::new(
                    legacy_path.map(StateFile::load).unwrap_or_else(StateFile::memory),
                )))
            }
        }
    }

    pub fn load(&self) -> Preferences {
        match &self.0 {
            Backend::GSettings { settings, .. } => read_settings(settings),
            Backend::Json(file) => file.read(Clone::clone).unwrap_or_default(),
        }
    }

    pub fn update(&self, mutate: impl FnOnce(&mut Preferences)) -> Result<(), String> {
        match &self.0 {
            Backend::GSettings {
                settings,
                legacy_path,
                migration_error,
            } => {
                if let Some(error) = migration_error {
                    return Err(format!("legacy preferences need recovery before editing: {error}"));
                }
                let settings = settings.as_ref();
                let previous = read_settings(settings);
                let mut updated = previous.clone();
                mutate(&mut updated);
                write_settings(settings, &updated)?;
                gio::Settings::sync();
                if let Some(path) = legacy_path
                    && let Err(error) = atomic_write_json(path, &updated)
                {
                    if let Err(rollback_error) = write_settings(settings, &previous) {
                        tracing::error!(%rollback_error, "preferences: failed to roll back GSettings after legacy mirror failure");
                    }
                    gio::Settings::sync();
                    return Err(format!("legacy preferences mirror failed: {error}"));
                }
                Ok(())
            }
            Backend::Json(file) => file.update(mutate),
        }
    }

    pub fn flush(&self) -> Result<(), String> {
        match &self.0 {
            Backend::GSettings { .. } => {
                gio::Settings::sync();
                Ok(())
            }
            Backend::Json(file) => file.flush(),
        }
    }

    #[cfg(test)]
    fn from_settings(settings: gio::Settings, legacy_path: Option<PathBuf>) -> Self {
        Self(Backend::GSettings {
            settings: Rc::new(settings),
            legacy_path,
            migration_error: None,
        })
    }

    fn from_settings_with_legacy(settings: gio::Settings, legacy_path: Option<PathBuf>) -> Self {
        let migration_error = migrate_json_preferences(&settings, legacy_path.as_deref())
            .err()
            .map(|error| {
                tracing::warn!(%error, "preferences: legacy migration deferred; source retained");
                error
            });
        Self(Backend::GSettings {
            settings: Rc::new(settings),
            legacy_path,
            migration_error,
        })
    }
}

fn settings_path() -> &'static str {
    settings_path_for_profile(crate::config::profile())
}

fn settings_path_for_profile(profile: crate::config::Profile) -> &'static str {
    if profile == crate::config::Profile::Development {
        "/com/tablepro/linux/Devel/"
    } else {
        "/com/tablepro/linux/"
    }
}

pub(super) fn settings() -> Option<gio::Settings> {
    open_settings().ok()
}

fn schema_directory() -> PathBuf {
    std::env::var_os("GSETTINGS_SCHEMA_DIR")
        .filter(|directory| !directory.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("TABLEPRO_GSETTINGS_SCHEMA_DIR")))
}

fn open_settings() -> Result<gio::Settings, String> {
    let parent = gio::SettingsSchemaSource::default()
        .ok_or_else(|| "default GSettings schema source is unavailable".to_string())?;
    let local_source = schema_directory();
    let source = if local_source.is_dir() {
        gio::SettingsSchemaSource::from_directory(&local_source, Some(&parent), false)
            .map_err(|error| error.to_string())?
    } else {
        parent.clone()
    };
    let schema = source
        .lookup(crate::config::APP_ID, true)
        .ok_or_else(|| format!("GSettings schema {} is not installed", crate::config::APP_ID))?;
    Ok(gio::Settings::new_full(
        &schema,
        None::<&gio::SettingsBackend>,
        Some(settings_path()),
    ))
}

fn read_settings(settings: &gio::Settings) -> Preferences {
    Preferences {
        default_page_size: settings.uint64("default-page-size"),
        confirm_destructive: settings.boolean("confirm-destructive"),
        editor_font_size: settings.uint("editor-font-size"),
        history_retention_days: settings.uint("history-retention-days"),
        query_timeout_secs: settings.uint("query-timeout-secs"),
        csv_include_header: settings.boolean("csv-include-header"),
        color_scheme: ColorSchemePref::from_key(settings.string("color-scheme").as_str()),
        theme: ThemePref::from_key(settings.string("theme").as_str()),
        editor_font_family: settings.string("editor-font-family").to_string(),
        editor_vim_mode: settings.boolean("editor-vim-mode"),
        null_display: NullDisplayPref::from_key(settings.string("null-display").as_str()),
        open_tables_in_preview_tab: settings.boolean("open-tables-in-preview-tab"),
    }
}

fn write_settings(settings: &gio::Settings, prefs: &Preferences) -> Result<(), String> {
    settings.delay();
    let writes = [
        settings.set_uint64("default-page-size", prefs.default_page_size),
        settings.set_boolean("confirm-destructive", prefs.confirm_destructive),
        settings.set_uint("editor-font-size", prefs.editor_font_size),
        settings.set_uint("history-retention-days", prefs.history_retention_days),
        settings.set_uint("query-timeout-secs", prefs.query_timeout_secs),
        settings.set_boolean("csv-include-header", prefs.csv_include_header),
        settings.set_string("color-scheme", prefs.color_scheme.key()),
        settings.set_string("theme", prefs.theme.key()),
        settings.set_string("editor-font-family", &prefs.editor_font_family),
        settings.set_boolean("editor-vim-mode", prefs.editor_vim_mode),
        settings.set_string("null-display", prefs.null_display.key()),
        settings.set_boolean("open-tables-in-preview-tab", prefs.open_tables_in_preview_tab),
    ];
    if let Some(error) = writes.into_iter().find_map(Result::err) {
        settings.revert();
        return Err(error.to_string());
    }
    settings.apply();
    Ok(())
}

fn migrate_json_preferences(settings: &gio::Settings, path: Option<&Path>) -> Result<(), String> {
    let version = settings.uint("migration-version");
    if version > SETTINGS_MIGRATION_VERSION {
        return Err(format!("preferences schema version {version} is newer than this build"));
    }
    if version == SETTINGS_MIGRATION_VERSION || settings.boolean("preferences-migrated") {
        sync_rollback_preferences(settings, path)?;
        return Ok(());
    }
    let legacy = match path {
        Some(path) => match std::fs::symlink_metadata(path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err("legacy preferences path is not a regular file".into());
            }
            Ok(_) => {
                let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
                let preferences = serde_json::from_slice::<Preferences>(&bytes).map_err(|error| error.to_string())?;
                Some((preferences, bytes, backup_path(path)?))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        },
        None => None,
    };

    if let Some((legacy, bytes, backup)) = legacy {
        preserve_migration_backup(&backup, &bytes)?;
        write_settings(settings, &legacy)?;
        gio::Settings::sync();
        if read_settings(settings) != legacy {
            return Err("migrated preferences did not verify against GSettings".into());
        }
    }
    settings
        .set_boolean("preferences-migrated", true)
        .map_err(|error| error.to_string())?;
    settings
        .set_uint("migration-version", SETTINGS_MIGRATION_VERSION)
        .map_err(|error| error.to_string())?;
    settings.apply();
    gio::Settings::sync();
    if settings.uint("migration-version") != SETTINGS_MIGRATION_VERSION {
        return Err("GSettings migration marker did not persist".into());
    }
    Ok(())
}

fn sync_rollback_preferences(settings: &gio::Settings, path: Option<&Path>) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err("legacy preferences path is not a regular file".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let legacy: Preferences = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if read_settings(settings) != legacy {
        write_settings(settings, &legacy)?;
        gio::Settings::sync();
        if read_settings(settings) != legacy {
            return Err("rollback preferences did not verify against GSettings".into());
        }
    }
    Ok(())
}

fn backup_path(source: &Path) -> Result<PathBuf, String> {
    if source.file_name().is_none() {
        return Err("legacy preferences path has no filename".into());
    }
    Ok(source.with_extension("before-gsettings.json"))
}

fn preserve_migration_backup(path: &Path, bytes: &[u8]) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err("preferences rollback backup is not a regular file".into());
        }
        Ok(_) => {
            let existing = std::fs::read(path).map_err(|error| error.to_string())?;
            if existing != bytes {
                return Err("preferences rollback backup differs from the migration source".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            atomic_write_bytes(path, bytes).map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(())
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;
