use std::path::PathBuf;

use crate::StorageError;

pub fn storage_dir_name() -> &'static str {
    if option_env!("TABLEPRO_PROFILE") == Some("development") {
        "tablepro-devel"
    } else {
        "tablepro"
    }
}

pub fn secret_schema() -> &'static str {
    if option_env!("TABLEPRO_PROFILE") == Some("development") {
        "com.tablepro.linux.Devel.Password"
    } else {
        "com.tablepro.linux.Password"
    }
}

pub fn config_path(filename: &str) -> Result<PathBuf, StorageError> {
    Ok(config_dir()?.join(filename))
}

pub fn connections_config_path() -> Result<PathBuf, StorageError> {
    Ok(append_connections_file(config_dir()?))
}

pub fn data_path(filename: &str) -> Result<PathBuf, StorageError> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| StorageError::Schema("neither XDG_DATA_HOME nor HOME is set".into()))?;
    Ok(base.join(storage_dir_name()).join(filename))
}

fn config_dir() -> Result<PathBuf, StorageError> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| StorageError::Schema("neither XDG_CONFIG_HOME nor HOME is set".into()))?;
    Ok(base.join(storage_dir_name()))
}

fn append_connections_file(mut dir: PathBuf) -> PathBuf {
    dir.push("connections");
    dir.set_extension("json");
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_paths_and_secrets_share_an_identity() {
        if option_env!("TABLEPRO_PROFILE") == Some("development") {
            assert_eq!(storage_dir_name(), "tablepro-devel");
            assert_eq!(secret_schema(), "com.tablepro.linux.Devel.Password");
        } else {
            assert_eq!(storage_dir_name(), "tablepro");
            assert_eq!(secret_schema(), "com.tablepro.linux.Password");
        }
    }

    #[test]
    fn connections_file_uses_stem_and_json_extension() {
        let path = append_connections_file(PathBuf::from("/tmp").join(storage_dir_name()));
        assert_eq!(path, PathBuf::from("/tmp").join(storage_dir_name()).join("connections.json"));
        assert_eq!(path.file_stem().and_then(|name| name.to_str()), Some("connections"));
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("json"));
    }
}
