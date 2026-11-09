use bookie_core::Value;

use super::{MAX_REPLACE_SNAPSHOT_BYTES, MAX_REPLACE_SNAPSHOT_ROWS, ReplaceError, retained_key_bytes};

pub struct SnapshotRow {
    pub position: u32,
    pub key: Vec<Value>,
    pub cells: Vec<(usize, Value)>,
}

#[derive(Default)]
pub struct ReplaceSnapshot {
    rows: Vec<SnapshotRow>,
    bytes: usize,
}

impl ReplaceSnapshot {
    pub fn push(&mut self, position: u32, key: &[Value], cells: &[(usize, &Value)]) -> Result<(), ReplaceError> {
        let bytes = cells.iter().fold(retained_key_bytes(key), |total, (_, value)| {
            total.saturating_add(retained_key_bytes(std::slice::from_ref(*value)))
        });
        let bytes = bytes
            .saturating_add(std::mem::size_of::<SnapshotRow>())
            .saturating_add(cells.len().saturating_mul(std::mem::size_of::<(usize, Value)>()));
        let bytes = bytes.saturating_mul(2);
        if self.rows.len() >= MAX_REPLACE_SNAPSHOT_ROWS || bytes > MAX_REPLACE_SNAPSHOT_BYTES.saturating_sub(self.bytes)
        {
            return Err(ReplaceError::Budget);
        }
        self.bytes += bytes;
        self.rows.push(SnapshotRow {
            position,
            key: key.to_vec(),
            cells: cells
                .iter()
                .map(|(column, value)| (*column, (*value).clone()))
                .collect(),
        });
        Ok(())
    }

    pub fn rows(&self) -> &[SnapshotRow] {
        &self.rows
    }
}
