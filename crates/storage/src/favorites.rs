use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::StorageError;
use crate::file_access::{lock_path, read_bounded, write_atomically};

const CURRENT_VERSION: u32 = 1;
const MAX_FAVORITES: usize = 500;
const MAX_FAVORITES_FILE_BYTES: usize = 16 * 1024 * 1024;
const MAX_FAVORITE_NAME_BYTES: usize = 512;
const MAX_FAVORITE_SQL_BYTES: usize = 1024 * 1024;
const MAX_DRIVER_ID_BYTES: usize = 128;
const MAX_FOLDERS: usize = 200;
const MAX_FOLDER_NAME_BYTES: usize = 256;
const MAX_KEYWORDS: usize = 32;
const MAX_KEYWORD_BYTES: usize = 64;
const KNOWN_FILE_FIELDS: &[&str] = &["version", "favorites", "folders"];
const KNOWN_FAVORITE_FIELDS: &[&str] = &[
    "id",
    "name",
    "sql",
    "driver_id",
    "connection_id",
    "created_at",
    "last_used_at",
    "folder_id",
    "keywords",
];
const KNOWN_FOLDER_FIELDS: &[&str] = &["id", "name"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedQuery {
    pub id: Uuid,
    pub name: String,
    pub sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

impl SavedQuery {
    pub fn new(name: String, sql: String, driver_id: Option<String>, connection_id: Option<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            sql,
            driver_id,
            connection_id,
            created_at: Utc::now(),
            last_used_at: None,
            folder_id: None,
            keywords: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedQueryFolder {
    pub id: Uuid,
    pub name: String,
}

impl SavedQueryFolder {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SavedQueryLibrary {
    pub folders: Vec<SavedQueryFolder>,
    pub favorites: Vec<SavedQuery>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FavoritesFile {
    version: u32,
    #[serde(default)]
    favorites: Vec<SavedQuery>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    folders: Vec<SavedQueryFolder>,
}

pub async fn load_favorites() -> Result<Vec<SavedQuery>, StorageError> {
    load_from(&favorites_path()?).await
}

pub async fn load_saved_query_library() -> Result<SavedQueryLibrary, StorageError> {
    load_library_from(&favorites_path()?).await
}

pub async fn save_favorite(favorite: SavedQuery) -> Result<Vec<SavedQuery>, StorageError> {
    save_favorite_to(&favorites_path()?, favorite).await
}

pub async fn delete_favorite(id: Uuid) -> Result<Vec<SavedQuery>, StorageError> {
    delete_favorite_from(&favorites_path()?, id).await
}

pub async fn touch_favorite(id: Uuid) -> Result<(), StorageError> {
    touch_favorite_from(&favorites_path()?, id).await
}

async fn save_favorite_to(path: &Path, favorite: SavedQuery) -> Result<Vec<SavedQuery>, StorageError> {
    validate_favorite(&favorite)?;
    let _guard = lock_path(path).await?;
    let (mut library, stored) = load_document(path).await?;
    upsert(&mut library.favorites, favorite)?;
    validate_library(&library)?;
    save_to_unlocked(path, &library, stored.as_ref()).await?;
    Ok(library.favorites)
}

async fn delete_favorite_from(path: &Path, id: Uuid) -> Result<Vec<SavedQuery>, StorageError> {
    let _guard = lock_path(path).await?;
    let (mut library, stored) = load_document(path).await?;
    library.favorites.retain(|favorite| favorite.id != id);
    save_to_unlocked(path, &library, stored.as_ref()).await?;
    Ok(library.favorites)
}

async fn touch_favorite_from(path: &Path, id: Uuid) -> Result<(), StorageError> {
    let _guard = lock_path(path).await?;
    let (mut library, stored) = load_document(path).await?;
    let Some(favorite) = library.favorites.iter_mut().find(|favorite| favorite.id == id) else {
        return Ok(());
    };
    favorite.last_used_at = Some(Utc::now());
    save_to_unlocked(path, &library, stored.as_ref()).await
}

pub async fn save_saved_query_folder(folder: SavedQueryFolder) -> Result<SavedQueryLibrary, StorageError> {
    save_folder_to(&favorites_path()?, folder).await
}

pub async fn delete_saved_query_folder(id: Uuid) -> Result<SavedQueryLibrary, StorageError> {
    delete_folder_from(&favorites_path()?, id).await
}

pub async fn organize_saved_query(
    id: Uuid,
    folder_id: Option<Uuid>,
    keywords: Vec<String>,
) -> Result<SavedQueryLibrary, StorageError> {
    save_query_organization_to(&favorites_path()?, id, folder_id, keywords).await
}

async fn save_folder_to(path: &Path, folder: SavedQueryFolder) -> Result<SavedQueryLibrary, StorageError> {
    validate_folder(&folder)?;
    let _guard = lock_path(path).await?;
    let (mut library, stored) = load_document(path).await?;
    if let Some(existing) = library.folders.iter_mut().find(|existing| existing.id == folder.id) {
        *existing = folder;
    } else {
        if library.folders.len() >= MAX_FOLDERS {
            return Err(StorageError::Schema(format!(
                "folders are limited to {MAX_FOLDERS} entries"
            )));
        }
        library.folders.push(folder);
    }
    validate_library(&library)?;
    save_to_unlocked(path, &library, stored.as_ref()).await?;
    Ok(library)
}

async fn delete_folder_from(path: &Path, id: Uuid) -> Result<SavedQueryLibrary, StorageError> {
    let _guard = lock_path(path).await?;
    let (mut library, stored) = load_document(path).await?;
    let Some(index) = library.folders.iter().position(|folder| folder.id == id) else {
        return Err(StorageError::NotFound);
    };
    library.folders.remove(index);
    for favorite in &mut library.favorites {
        if favorite.folder_id == Some(id) {
            favorite.folder_id = None;
        }
    }
    save_to_unlocked(path, &library, stored.as_ref()).await?;
    Ok(library)
}

async fn save_query_organization_to(
    path: &Path,
    id: Uuid,
    folder_id: Option<Uuid>,
    keywords: Vec<String>,
) -> Result<SavedQueryLibrary, StorageError> {
    validate_keywords(&keywords)?;
    let _guard = lock_path(path).await?;
    let (mut library, stored) = load_document(path).await?;
    if let Some(folder_id) = folder_id
        && !library.folders.iter().any(|folder| folder.id == folder_id)
    {
        return Err(StorageError::NotFound);
    }
    let Some(favorite) = library.favorites.iter_mut().find(|favorite| favorite.id == id) else {
        return Err(StorageError::NotFound);
    };
    favorite.folder_id = folder_id;
    favorite.keywords = keywords;
    save_to_unlocked(path, &library, stored.as_ref()).await?;
    Ok(library)
}

pub(crate) fn upsert(existing: &mut Vec<SavedQuery>, favorite: SavedQuery) -> Result<(), StorageError> {
    validate_favorite(&favorite)?;
    if let Some(slot) = existing.iter_mut().find(|candidate| candidate.id == favorite.id) {
        *slot = favorite;
        return Ok(());
    }
    if let Some(slot) = existing
        .iter_mut()
        .find(|candidate| candidate.name.eq_ignore_ascii_case(favorite.name.trim()))
    {
        let id = slot.id;
        let created_at = slot.created_at;
        let last_used_at = slot.last_used_at;
        let folder_id = slot.folder_id;
        let keywords = std::mem::take(&mut slot.keywords);
        *slot = SavedQuery {
            id,
            created_at,
            last_used_at,
            folder_id,
            keywords,
            ..favorite
        };
        return Ok(());
    }
    if existing.len() >= MAX_FAVORITES {
        return Err(StorageError::Schema(format!(
            "favorites are limited to {MAX_FAVORITES} entries"
        )));
    }
    existing.push(favorite);
    Ok(())
}

fn validate_favorite(favorite: &SavedQuery) -> Result<(), StorageError> {
    validate_required_text(
        &favorite.name,
        MAX_FAVORITE_NAME_BYTES,
        "a favorite needs a name",
        "favorite name",
    )?;
    validate_required_text(
        &favorite.sql,
        MAX_FAVORITE_SQL_BYTES,
        "a favorite needs a statement",
        "favorite statement",
    )?;
    if let Some(driver_id) = favorite.driver_id.as_deref()
        && driver_id.len() > MAX_DRIVER_ID_BYTES
    {
        return Err(StorageError::TooLarge {
            got: driver_id.len(),
            limit: MAX_DRIVER_ID_BYTES,
        });
    }
    validate_keywords(&favorite.keywords)?;
    Ok(())
}

fn validate_folder(folder: &SavedQueryFolder) -> Result<(), StorageError> {
    validate_required_text(
        &folder.name,
        MAX_FOLDER_NAME_BYTES,
        "a folder needs a name",
        "folder name",
    )
}

fn validate_keywords(keywords: &[String]) -> Result<(), StorageError> {
    if keywords.len() > MAX_KEYWORDS {
        return Err(StorageError::Schema(format!(
            "keywords are limited to {MAX_KEYWORDS} per query"
        )));
    }
    let mut seen = std::collections::HashSet::new();
    for keyword in keywords {
        validate_required_text(keyword, MAX_KEYWORD_BYTES, "a keyword cannot be empty", "keyword")?;
        if !seen.insert(keyword.to_lowercase()) {
            return Err(StorageError::Schema("keywords must be unique".into()));
        }
    }
    Ok(())
}

fn validate_library(library: &SavedQueryLibrary) -> Result<(), StorageError> {
    if library.favorites.len() > MAX_FAVORITES {
        return Err(StorageError::Schema(format!(
            "favorites are limited to {MAX_FAVORITES} entries"
        )));
    }
    if library.folders.len() > MAX_FOLDERS {
        return Err(StorageError::Schema(format!(
            "folders are limited to {MAX_FOLDERS} entries"
        )));
    }
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for folder in &library.folders {
        validate_folder(folder)?;
        if !ids.insert(folder.id) || !names.insert(folder.name.trim().to_lowercase()) {
            return Err(StorageError::Schema(
                "folder identifiers and names must be unique".into(),
            ));
        }
    }
    for favorite in &library.favorites {
        validate_favorite(favorite)?;
        if favorite.folder_id.is_some_and(|id| !ids.contains(&id)) {
            return Err(StorageError::Schema("saved query references a missing folder".into()));
        }
    }
    Ok(())
}

fn validate_required_text(value: &str, limit: usize, empty_error: &str, field: &str) -> Result<(), StorageError> {
    if value.trim().is_empty() {
        return Err(StorageError::Schema(empty_error.into()));
    }
    if value.len() > limit {
        return Err(StorageError::Schema(format!("{field} exceeds {limit} bytes")));
    }
    Ok(())
}

pub fn rank_favorites(favorites: &[SavedQuery]) -> Vec<SavedQuery> {
    let mut ordered = favorites.to_vec();
    ordered.sort_by(|left, right| {
        right
            .last_used_at
            .cmp(&left.last_used_at)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    ordered
}

pub fn matches_filter(favorite: &SavedQuery, filter: &str) -> bool {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    favorite.name.to_lowercase().contains(&needle)
        || favorite.sql.to_lowercase().contains(&needle)
        || favorite
            .keywords
            .iter()
            .any(|keyword| keyword.to_lowercase().contains(&needle))
}

pub(crate) async fn load_from(path: &Path) -> Result<Vec<SavedQuery>, StorageError> {
    load_document(path).await.map(|(library, _)| library.favorites)
}

async fn load_library_from(path: &Path) -> Result<SavedQueryLibrary, StorageError> {
    load_document(path).await.map(|(library, _)| library)
}

async fn load_document(
    path: &Path,
) -> Result<(SavedQueryLibrary, Option<serde_json::Map<String, serde_json::Value>>), StorageError> {
    let Some(bytes) = read_bounded(path, MAX_FAVORITES_FILE_BYTES).await? else {
        return Ok((SavedQueryLibrary::default(), None));
    };
    let stored: serde_json::Value = serde_json::from_slice(&bytes)?;
    let stored = stored
        .as_object()
        .cloned()
        .ok_or_else(|| StorageError::Schema("favorites.json must contain an object".into()))?;
    let file: FavoritesFile = serde_json::from_value(serde_json::Value::Object(stored.clone()))?;
    if file.version != CURRENT_VERSION {
        return Err(StorageError::Schema(format!(
            "favorites.json version {} not supported (expected {})",
            file.version, CURRENT_VERSION,
        )));
    }
    let library = SavedQueryLibrary {
        favorites: file.favorites,
        folders: file.folders,
    };
    validate_library(&library)?;
    Ok((library, Some(stored)))
}

#[cfg(test)]
pub(crate) async fn save_to(path: &Path, favorites: &[SavedQuery]) -> Result<(), StorageError> {
    for favorite in favorites {
        validate_favorite(favorite)?;
    }
    if favorites.len() > MAX_FAVORITES {
        return Err(StorageError::Schema(format!(
            "favorites are limited to {MAX_FAVORITES} entries"
        )));
    }
    let _guard = lock_path(path).await?;
    let (mut library, stored) = load_document(path).await?;
    library.favorites = favorites.to_vec();
    save_to_unlocked(path, &library, stored.as_ref()).await
}

async fn save_to_unlocked(
    path: &Path,
    library: &SavedQueryLibrary,
    stored: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<(), StorageError> {
    validate_library(library)?;
    let file = FavoritesFile {
        version: CURRENT_VERSION,
        favorites: library.favorites.clone(),
        folders: library.folders.clone(),
    };
    let mut document = serde_json::to_value(file)?;
    if let Some(stored) = stored {
        carry_unknown_fields(&mut document, stored);
    }
    let json = serde_json::to_vec_pretty(&document)?;
    if json.len() > MAX_FAVORITES_FILE_BYTES {
        return Err(StorageError::TooLarge {
            got: json.len(),
            limit: MAX_FAVORITES_FILE_BYTES,
        });
    }
    write_atomically(path, &json).await
}

fn carry_unknown_fields(document: &mut serde_json::Value, stored: &serde_json::Map<String, serde_json::Value>) {
    let Some(fresh) = document.as_object_mut() else {
        return;
    };
    for (key, value) in stored {
        if !KNOWN_FILE_FIELDS.contains(&key.as_str()) {
            fresh.insert(key.clone(), value.clone());
        }
    }
    if let (Some(stored_entries), Some(fresh_entries)) = (
        stored.get("favorites").and_then(serde_json::Value::as_array),
        fresh.get_mut("favorites").and_then(serde_json::Value::as_array_mut),
    ) {
        for entry in fresh_entries {
            let Some(id) = entry.get("id").and_then(serde_json::Value::as_str).map(str::to_owned) else {
                continue;
            };
            let Some(previous) = stored_entries
                .iter()
                .filter_map(serde_json::Value::as_object)
                .find(|candidate| candidate.get("id").and_then(serde_json::Value::as_str) == Some(id.as_str()))
            else {
                continue;
            };
            let Some(target) = entry.as_object_mut() else {
                continue;
            };
            for (key, value) in previous {
                if !KNOWN_FAVORITE_FIELDS.contains(&key.as_str()) {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
    }
    let Some(stored_folders) = stored.get("folders").and_then(serde_json::Value::as_array) else {
        return;
    };
    let Some(fresh_folders) = fresh.get_mut("folders").and_then(serde_json::Value::as_array_mut) else {
        return;
    };
    for entry in fresh_folders {
        let Some(id) = entry.get("id").and_then(serde_json::Value::as_str).map(str::to_owned) else {
            continue;
        };
        let Some(previous) = stored_folders
            .iter()
            .filter_map(serde_json::Value::as_object)
            .find(|candidate| candidate.get("id").and_then(serde_json::Value::as_str) == Some(id.as_str()))
        else {
            continue;
        };
        let Some(target) = entry.as_object_mut() else {
            continue;
        };
        for (key, value) in previous {
            if !KNOWN_FOLDER_FIELDS.contains(&key.as_str()) {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn favorites_path() -> Result<PathBuf, StorageError> {
    crate::config_path("favorites.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn favorite(name: &str, sql: &str) -> SavedQuery {
        SavedQuery::new(name.to_string(), sql.to_string(), Some("postgres".into()), None)
    }

    #[test]
    fn a_favorite_needs_a_name_and_a_statement() {
        let mut existing = Vec::new();
        assert!(upsert(&mut existing, favorite("  ", "SELECT 1")).is_err());
        assert!(upsert(&mut existing, favorite("name", "   ")).is_err());
        assert!(existing.is_empty());
    }

    #[test]
    fn saving_the_same_name_replaces_the_statement_and_keeps_the_id() {
        let mut existing = Vec::new();
        upsert(&mut existing, favorite("Daily", "SELECT 1")).expect("first save");
        let id = existing[0].id;
        let created_at = existing[0].created_at;
        let last_used_at = Some(Utc::now());
        existing[0].last_used_at = last_used_at;

        upsert(&mut existing, favorite("daily", "SELECT 2")).expect("second save");

        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0].id, id);
        assert_eq!(existing[0].sql, "SELECT 2");
        assert_eq!(existing[0].created_at, created_at);
        assert_eq!(existing[0].last_used_at, last_used_at);
    }

    #[test]
    fn saving_the_same_id_updates_in_place() {
        let mut existing = Vec::new();
        upsert(&mut existing, favorite("Daily", "SELECT 1")).expect("first save");
        let mut edited = existing[0].clone();
        edited.name = "Renamed".into();

        upsert(&mut existing, edited).expect("edit");

        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0].name, "Renamed");
    }

    #[test]
    fn favorites_are_capped() {
        let mut existing: Vec<SavedQuery> = (0..MAX_FAVORITES)
            .map(|index| favorite(&format!("q{index}"), "SELECT 1"))
            .collect();
        assert!(upsert(&mut existing, favorite("one more", "SELECT 1")).is_err());
        assert_eq!(existing.len(), MAX_FAVORITES);
    }

    #[test]
    fn ranking_puts_recently_used_first_then_sorts_by_name() {
        let mut recent = favorite("zeta", "SELECT 1");
        recent.last_used_at = Some(Utc::now());
        let older = favorite("alpha", "SELECT 2");
        let middle = favorite("beta", "SELECT 3");

        let ranked = rank_favorites(&[older, middle, recent]);

        assert_eq!(ranked[0].name, "zeta");
        assert_eq!(ranked[1].name, "alpha");
        assert_eq!(ranked[2].name, "beta");
    }

    #[test]
    fn filtering_matches_name_and_statement_case_insensitively() {
        let mut entry = favorite("Daily revenue", "SELECT sum(total) FROM orders");
        entry.keywords = vec!["finance".into()];
        assert!(matches_filter(&entry, ""));
        assert!(matches_filter(&entry, "daily"));
        assert!(matches_filter(&entry, "ORDERS"));
        assert!(matches_filter(&entry, "FINANCE"));
        assert!(!matches_filter(&entry, "customers"));
    }

    #[tokio::test]
    async fn favorites_round_trip_through_a_file() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");

        let entries = vec![favorite("Daily", "SELECT 1")];
        save_to(&path, &entries).await.expect("save");
        let loaded = load_from(&path).await.expect("load");

        assert_eq!(loaded, entries);
    }

    #[tokio::test]
    async fn flat_v1_queries_load_without_folder_metadata() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let entry = favorite("Daily", "SELECT 1");
        let document = serde_json::json!({
            "version": 1,
            "favorites": [{
                "id": entry.id,
                "name": entry.name,
                "sql": entry.sql,
                "driver_id": entry.driver_id,
                "created_at": entry.created_at,
                "last_used_at": entry.last_used_at
            }]
        });
        tokio::fs::write(&path, serde_json::to_vec(&document).expect("serialize"))
            .await
            .expect("write");

        let library = load_library_from(&path).await.expect("load old flat file");

        assert!(library.folders.is_empty());
        assert_eq!(library.favorites.len(), 1);
        assert_eq!(library.favorites[0].id, entry.id);
        assert_eq!(library.favorites[0].sql, "SELECT 1");
        assert_eq!(library.favorites[0].folder_id, None);
        assert!(library.favorites[0].keywords.is_empty());
    }

    #[tokio::test]
    async fn folders_and_query_metadata_round_trip() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let folder = SavedQueryFolder::new("Reports".into());
        save_folder_to(&path, folder.clone()).await.expect("create folder");
        let entry = favorite("Daily", "SELECT 1");
        save_favorite_to(&path, entry.clone()).await.expect("save query");
        save_query_organization_to(&path, entry.id, Some(folder.id), vec!["daily".into(), "finance".into()])
            .await
            .expect("organize query");

        let loaded = load_library_from(&path).await.expect("reload library");

        assert_eq!(loaded.folders, vec![folder]);
        assert_eq!(loaded.favorites[0].folder_id, Some(loaded.folders[0].id));
        assert_eq!(loaded.favorites[0].keywords, ["daily", "finance"]);
    }

    #[tokio::test]
    async fn deleting_a_folder_keeps_its_queries_and_keywords_unfiled() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let folder = SavedQueryFolder::new("Reports".into());
        save_folder_to(&path, folder.clone()).await.expect("create folder");
        let mut entry = favorite("Daily", "SELECT 1");
        entry.last_used_at = Some(Utc::now());
        save_favorite_to(&path, entry.clone()).await.expect("save query");
        save_query_organization_to(&path, entry.id, Some(folder.id), vec!["finance".into()])
            .await
            .expect("organize query");

        let loaded = delete_folder_from(&path, folder.id).await.expect("delete folder");

        assert!(loaded.folders.is_empty());
        assert_eq!(loaded.favorites[0].id, entry.id);
        assert_eq!(loaded.favorites[0].sql, entry.sql);
        assert_eq!(loaded.favorites[0].last_used_at, entry.last_used_at);
        assert_eq!(loaded.favorites[0].folder_id, None);
        assert_eq!(loaded.favorites[0].keywords, ["finance"]);
    }

    #[tokio::test]
    async fn assigning_a_missing_folder_refuses_without_replacing_the_file() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let entry = favorite("Daily", "SELECT 1");
        save_favorite_to(&path, entry.clone()).await.expect("save query");
        let before = tokio::fs::read(&path).await.expect("read before");

        let result = save_query_organization_to(&path, entry.id, Some(Uuid::new_v4()), vec!["finance".into()]).await;

        assert!(result.is_err());
        assert_eq!(tokio::fs::read(&path).await.expect("read after"), before);
    }

    #[tokio::test]
    async fn saving_a_new_query_with_an_existing_name_keeps_its_organization() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let folder = SavedQueryFolder::new("Reports".into());
        save_folder_to(&path, folder.clone()).await.expect("create folder");
        let existing = favorite("Daily", "SELECT 1");
        save_favorite_to(&path, existing.clone()).await.expect("save old query");
        save_query_organization_to(&path, existing.id, Some(folder.id), vec!["finance".into()])
            .await
            .expect("organize old query");
        let replacement = favorite(" daily ", "SELECT 2");

        save_favorite_to(&path, replacement).await.expect("replace by name");
        let loaded = load_library_from(&path).await.expect("reload library");

        assert_eq!(loaded.favorites.len(), 1);
        assert_eq!(loaded.favorites[0].id, existing.id);
        assert_eq!(loaded.favorites[0].sql, "SELECT 2");
        assert_eq!(loaded.favorites[0].folder_id, Some(folder.id));
        assert_eq!(loaded.favorites[0].keywords, ["finance"]);
    }

    #[tokio::test]
    async fn renaming_a_folder_preserves_unknown_folder_fields() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let mut folder = SavedQueryFolder::new("Reports".into());
        save_folder_to(&path, folder.clone()).await.expect("create folder");
        let mut document: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read")).expect("json");
        document["folders"][0]["future_folder_field"] = serde_json::json!("keep");
        tokio::fs::write(&path, serde_json::to_vec_pretty(&document).expect("serialize"))
            .await
            .expect("write");
        folder.name = "Renamed".into();

        save_folder_to(&path, folder).await.expect("rename folder");

        let rewritten: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read")).expect("json");
        assert_eq!(rewritten["folders"][0]["future_folder_field"], "keep");
        assert_eq!(rewritten["folders"][0]["name"], "Renamed");
    }

    #[tokio::test]
    async fn duplicate_folder_names_and_dangling_references_block_writes() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let first = SavedQueryFolder::new("Reports".into());
        save_folder_to(&path, first.clone()).await.expect("create folder");
        let before = tokio::fs::read(&path).await.expect("read before");

        let duplicate = save_folder_to(&path, SavedQueryFolder::new(" reports ".into())).await;

        assert!(duplicate.is_err());
        assert_eq!(tokio::fs::read(&path).await.expect("read after"), before);
        let entry = favorite("Daily", "SELECT 1");
        save_favorite_to(&path, entry.clone()).await.expect("save query");
        let before = tokio::fs::read(&path).await.expect("read before");
        let mut invalid = entry;
        invalid.folder_id = Some(Uuid::new_v4());

        assert!(save_favorite_to(&path, invalid).await.is_err());
        assert_eq!(tokio::fs::read(&path).await.expect("read after"), before);
    }

    #[tokio::test]
    async fn duplicate_folder_ids_in_the_file_block_updates_without_replacing_it() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let folder = SavedQueryFolder::new("Reports".into());
        save_folder_to(&path, folder.clone()).await.expect("create folder");
        let mut document: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read")).expect("json");
        document["folders"]
            .as_array_mut()
            .expect("folders array")
            .push(serde_json::json!({
                "id": folder.id,
                "name": "Duplicate"
            }));
        let malformed = serde_json::to_vec_pretty(&document).expect("serialize");
        tokio::fs::write(&path, &malformed).await.expect("write");

        assert!(save_favorite_to(&path, favorite("Daily", "SELECT 1")).await.is_err());
        assert_eq!(tokio::fs::read(&path).await.expect("read"), malformed);
    }

    #[tokio::test]
    async fn keyword_limits_refuse_writes_without_changing_the_saved_queries() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let entry = favorite("Daily", "SELECT 1");
        save_favorite_to(&path, entry.clone()).await.expect("save query");
        let before = tokio::fs::read(&path).await.expect("read before");
        let too_many = vec!["tag".to_string(); MAX_KEYWORDS + 1];

        assert!(
            save_query_organization_to(&path, entry.id, None, too_many)
                .await
                .is_err()
        );
        assert_eq!(tokio::fs::read(&path).await.expect("read after too many"), before);
        assert!(
            save_query_organization_to(&path, entry.id, None, vec!["x".repeat(MAX_KEYWORD_BYTES + 1)],)
                .await
                .is_err()
        );
        assert_eq!(tokio::fs::read(&path).await.expect("read after long"), before);
    }

    #[tokio::test]
    async fn touching_a_query_preserves_its_folder_and_keywords() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let folder = SavedQueryFolder::new("Reports".into());
        save_folder_to(&path, folder.clone()).await.expect("create folder");
        let entry = favorite("Daily", "SELECT 1");
        save_favorite_to(&path, entry.clone()).await.expect("save query");
        save_query_organization_to(&path, entry.id, Some(folder.id), vec!["finance".into()])
            .await
            .expect("organize query");

        touch_favorite_from(&path, entry.id).await.expect("touch query");

        let library = load_library_from(&path).await.expect("reload library");
        assert_eq!(library.favorites[0].folder_id, Some(folder.id));
        assert_eq!(library.favorites[0].keywords, ["finance"]);
        assert!(library.favorites[0].last_used_at.is_some());
    }

    #[tokio::test]
    async fn a_missing_file_loads_as_empty() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let loaded = load_from(&dir.path().join("favorites.json")).await.expect("load");
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn an_unsupported_version_is_rejected() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        tokio::fs::write(&path, br#"{"version":99,"favorites":[]}"#)
            .await
            .expect("write");

        assert!(load_from(&path).await.is_err());
    }

    #[tokio::test]
    async fn a_malformed_file_is_preserved_and_blocks_a_save() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let malformed = b"{not valid json";
        tokio::fs::write(&path, malformed).await.expect("write");

        assert!(save_favorite_to(&path, favorite("Daily", "SELECT 1")).await.is_err());
        assert_eq!(tokio::fs::read(&path).await.expect("read"), malformed);
    }

    #[tokio::test]
    async fn unknown_file_and_favorite_fields_survive_an_update() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("favorites.json");
        let mut entry = favorite("Daily", "SELECT 1");
        save_to(&path, std::slice::from_ref(&entry))
            .await
            .expect("initial save");
        let mut document: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read")).expect("json");
        document["future_section"] = serde_json::json!({ "keep": true });
        document["favorites"][0]["display_color"] = serde_json::json!("teal");
        tokio::fs::write(&path, serde_json::to_vec_pretty(&document).expect("serialize"))
            .await
            .expect("write");
        entry.sql = "SELECT 2".into();

        save_favorite_to(&path, entry).await.expect("update");

        let rewritten: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.expect("read")).expect("json");
        assert_eq!(rewritten["future_section"]["keep"], true);
        assert_eq!(rewritten["favorites"][0]["display_color"], "teal");
        assert_eq!(rewritten["favorites"][0]["sql"], "SELECT 2");
    }
}
