use std::path::Path;

use tablepro_policy::{PolicyConfig, load_from_path};

use crate::services::config_io::xdg_config_path;

pub(super) fn load_policy() -> Result<PolicyConfig, String> {
    let path = xdg_config_path("policy.toml").ok_or("neither XDG_CONFIG_HOME nor HOME is set")?;
    load_policy_file(&path)
}

pub(super) fn load_policy_file(path: &Path) -> Result<PolicyConfig, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => load_from_path(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            validate_missing_parent(path).map_err(|error| error.to_string())?;
            Ok(PolicyConfig::default())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn validate_missing_parent(path: &Path) -> std::io::Result<()> {
    for parent in path.ancestors().skip(1) {
        match std::fs::symlink_metadata(parent) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    std::fs::metadata(parent)?;
                }
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
