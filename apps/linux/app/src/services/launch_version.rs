use gtk4::gio;
use gtk4::gio::prelude::*;

const KEY: &str = "last-launched-version";

pub fn last_launched_version() -> Option<String> {
    let settings = super::preferences::settings()?;
    let value = settings.string(KEY);
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub fn store_last_launched_version(version: &str) -> Result<(), String> {
    let settings = super::preferences::settings().ok_or_else(|| {
        bookie_app_services::tr!("Could not open settings to remember the launched version.").to_string()
    })?;
    settings
        .set_string(KEY, version)
        .map_err(|error| error.to_string())?;
    gio::Settings::sync();
    Ok(())
}
