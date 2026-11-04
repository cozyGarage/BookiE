use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::config_io::xdg_config_path;
use super::state_file::StateFile;

const FILE: &str = "sql_snippets.json";
pub const MAX_SNIPPETS: usize = 200;
pub const MAX_NAME_CHARS: usize = 48;
pub const MAX_BODY_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    pub text: String,
    pub cursor: usize,
}

#[derive(Clone)]
pub struct SnippetStore {
    file: Arc<StateFile<HashMap<String, Snippet>>>,
}

impl SnippetStore {
    pub fn open() -> Option<Self> {
        Some(Self::load_at(xdg_config_path(FILE)?))
    }

    pub fn load_at(path: std::path::PathBuf) -> Self {
        Self {
            file: Arc::new(StateFile::load(path)),
        }
    }

    pub fn flush(&self) -> Result<(), String> {
        self.file.flush()
    }

    pub fn list(&self) -> Vec<Snippet> {
        let mut snippets = self
            .file
            .read(|map| {
                map.iter()
                    .filter(|(key, snippet)| is_valid(key, snippet))
                    .map(|(_, snippet)| snippet.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        snippets.sort_by_key(|snippet| snippet.name.to_lowercase());
        snippets
    }

    pub fn get(&self, name: &str) -> Option<Snippet> {
        let key = valid_name(name)?.to_lowercase();
        self.file
            .read(|map| map.get(&key).filter(|snippet| is_valid(&key, snippet)).cloned())?
    }

    pub fn save(&self, name: &str, body: &str) -> Result<(), String> {
        let name = valid_name(name).ok_or_else(|| "Choose a name of 1 to 48 characters.".to_string())?;
        if body.trim().is_empty() || body.len() > MAX_BODY_BYTES {
            return Err("A snippet needs text and at most 16 KiB of it.".into());
        }
        let key = name.to_lowercase();
        let exists = self.file.read(|map| map.contains_key(&key)).unwrap_or(false);
        let count = self.file.read(HashMap::len).unwrap_or(0);
        if !exists && count >= MAX_SNIPPETS {
            return Err("There is already the maximum number of snippets.".into());
        }
        let snippet = Snippet {
            name,
            body: body.to_string(),
        };
        self.file.update(|map| {
            map.insert(key, snippet);
        })
    }

    pub fn delete(&self, name: &str) -> Result<bool, String> {
        let key = valid_name(name)
            .ok_or_else(|| "Choose a snippet to delete.".to_string())?
            .to_lowercase();
        let mut removed = false;
        self.file.update(|map| removed = map.remove(&key).is_some())?;
        Ok(removed)
    }
}

pub fn expand(body: &str) -> Expansion {
    let mut text = String::with_capacity(body.len());
    let mut cursor = None;
    let mut chars = body.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '$' {
            text.push(character);
            continue;
        }
        match chars.peek() {
            Some('$') => {
                chars.next();
                text.push('$');
            }
            Some('0') => {
                chars.next();
                cursor.get_or_insert(text.len());
            }
            _ => text.push('$'),
        }
    }
    let cursor = cursor.unwrap_or(text.len());
    Expansion { text, cursor }
}

fn valid_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && name.chars().count() <= MAX_NAME_CHARS && !name.chars().any(char::is_control))
        .then(|| name.to_string())
}

fn is_valid(key: &str, snippet: &Snippet) -> bool {
    valid_name(&snippet.name).is_some_and(|name| name.to_lowercase() == key)
        && !snippet.body.trim().is_empty()
        && snippet.body.len() <= MAX_BODY_BYTES
}

#[cfg(test)]
#[path = "snippets_tests.rs"]
mod tests;
