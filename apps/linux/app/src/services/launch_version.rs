use gtk4::gio;
use gtk4::gio::prelude::*;

const LAST_LAUNCHED_KEY: &str = "last-launched-version";
const DISMISSED_UPDATE_KEY: &str = "dismissed-update-version";

pub fn last_launched_version() -> Option<String> {
    read_string(LAST_LAUNCHED_KEY)
}

pub fn store_last_launched_version(version: &str) -> Result<(), String> {
    write_string(
        LAST_LAUNCHED_KEY,
        version,
        bookie_app_services::tr!("Could not open settings to remember the launched version."),
    )
}

pub fn dismissed_update_version() -> Option<String> {
    read_string(DISMISSED_UPDATE_KEY)
}

pub fn store_dismissed_update_version(version: &str) -> Result<(), String> {
    write_string(
        DISMISSED_UPDATE_KEY,
        version,
        bookie_app_services::tr!("Could not open settings to remember the update choice."),
    )
}

fn read_string(key: &str) -> Option<String> {
    let settings = super::preferences::settings()?;
    let value = settings.string(key);
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn write_string(key: &str, value: &str, missing_settings: impl Into<String>) -> Result<(), String> {
    let settings = super::preferences::settings().ok_or_else(|| missing_settings.into())?;
    settings
        .set_string(key, value)
        .map_err(|error| error.to_string())?;
    gio::Settings::sync();
    Ok(())
}
