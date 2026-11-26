use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

const FILE_LIMIT: u64 = 1024 * 1024;
const MAX_COORDINATE: f64 = 100_000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardPlacement {
    pub schema: Option<String>,
    pub table: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
}

impl CardPlacement {
    pub fn valid(&self) -> bool {
        (0.0..=MAX_COORDINATE).contains(&self.x)
            && (0.0..=MAX_COORDINATE).contains(&self.y)
            && (120.0..=600.0).contains(&self.width)
    }
}

pub fn load(path: &Path) -> io::Result<Vec<CardPlacement>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(io::Error::other("diagram layout is too large"));
    }
    let records: Vec<CardPlacement> = serde_json::from_slice(&bytes)?;
    Ok(records
        .into_iter()
        .filter(CardPlacement::valid)
        .take(super::read::MAX_TABLES)
        .collect())
}

pub fn save(path: &Path, records: &[CardPlacement]) -> io::Result<()> {
    crate::config_io::atomic_write_json(path, &records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_round_trips_and_drops_an_invalid_card() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("layout.json");
        let placed = CardPlacement {
            schema: Some("sales".into()),
            table: "orders".into(),
            x: 150.0,
            y: 45.0,
            width: 250.0,
        };
        let outside = CardPlacement {
            x: -5.0,
            table: "returns".into(),
            ..placed.clone()
        };
        save(&path, &[placed.clone(), outside]).unwrap();
        assert_eq!(load(&path).unwrap(), vec![placed]);
    }
}
