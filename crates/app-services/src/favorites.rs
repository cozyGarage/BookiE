use std::path::{Path, PathBuf};

use bookie_storage::SavedQuery;
use uuid::Uuid;

const NAME_CHARS: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favorite {
    pub id: Uuid,
    pub name: String,
    pub sql: String,
}

pub fn default_name(sql: &str) -> String {
    let line = sql.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    let mut name: String = line.chars().take(NAME_CHARS).collect();
    if line.chars().count() > NAME_CHARS {
        name.push('…');
    }
    name
}

pub fn list(path: Option<&Path>) -> Result<Vec<Favorite>, String> {
    let stored = run(async move {
        match path_of(path) {
            Some(path) => bookie_storage::load_favorites_from(&path).await,
            None => bookie_storage::load_favorites().await,
        }
    })?;
    Ok(ranked(&stored))
}

pub fn save(path: Option<&Path>, sql: &str, driver_id: Option<String>) -> Result<Vec<Favorite>, String> {
    if sql.trim().is_empty() {
        return Err("Type a statement first".into());
    }
    let favorite = SavedQuery::new(default_name(sql), sql.to_owned(), driver_id, None);
    let stored = run(async move {
        match path_of(path) {
            Some(path) => bookie_storage::save_favorite_to(&path, favorite).await,
            None => bookie_storage::save_favorite(favorite).await,
        }
    })?;
    Ok(ranked(&stored))
}

pub fn delete(path: Option<&Path>, id: Uuid) -> Result<Vec<Favorite>, String> {
    let stored = run(async move {
        match path_of(path) {
            Some(path) => bookie_storage::delete_favorite_from(&path, id).await,
            None => bookie_storage::delete_favorite(id).await,
        }
    })?;
    Ok(ranked(&stored))
}

fn path_of(path: Option<&Path>) -> Option<PathBuf> {
    path.map(Path::to_path_buf)
}

fn ranked(stored: &[SavedQuery]) -> Vec<Favorite> {
    bookie_storage::rank_favorites(stored)
        .into_iter()
        .map(|favorite| Favorite {
            id: favorite.id,
            name: favorite.name,
            sql: favorite.sql,
        })
        .collect()
}

fn run<T>(work: impl std::future::Future<Output = Result<T, bookie_storage::StorageError>>) -> Result<T, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(work).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_the_first_non_empty_line_cut_to_a_readable_length() {
        assert_eq!(default_name("\n  SELECT 1\nFROM t"), "SELECT 1");
        assert_eq!(default_name(""), "");
        let long = format!("SELECT {}", "x".repeat(100));
        let name = default_name(&long);
        assert_eq!(name.chars().count(), NAME_CHARS + 1);
        assert!(name.ends_with('…'));
    }

    #[test]
    fn favorites_are_saved_listed_and_deleted_in_the_given_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = directory.path().join("favorites.json");
        assert!(list(Some(&file)).expect("empty list").is_empty());
        assert_eq!(save(Some(&file), "  ", None), Err("Type a statement first".into()));
        let saved = save(Some(&file), "SELECT 1", Some("sqlite".into())).expect("save");
        assert_eq!(saved.len(), 1);
        assert_eq!(
            (saved[0].name.as_str(), saved[0].sql.as_str()),
            ("SELECT 1", "SELECT 1")
        );
        let again = list(Some(&file)).expect("list");
        assert_eq!(again, saved);
        assert!(delete(Some(&file), saved[0].id).expect("delete").is_empty());
        assert!(list(Some(&file)).expect("list").is_empty());
    }
}
