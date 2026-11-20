use std::path::{Path, PathBuf};

pub fn expand_tilde(path: &Path) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    expand_tilde_with(path, home.as_deref())
}

pub fn expand_tilde_with(path: &Path, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~"), home) {
        (Ok(rest), Some(home)) if rest.as_os_str().is_empty() => home.to_path_buf(),
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
#[path = "user_path_tests.rs"]
mod tests;
