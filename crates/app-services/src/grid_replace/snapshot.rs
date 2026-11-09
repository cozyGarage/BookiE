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
        self.push_row(position, key, |visit| {
            for (column, value) in cells {
                visit(*column, value);
            }
        })
    }

    pub fn push_row(
        &mut self,
        position: u32,
        key: &[Value],
        visit_cells: impl FnOnce(&mut dyn FnMut(usize, &Value)),
    ) -> Result<(), ReplaceError> {
        self.push_row_cancellable(position, key, None, visit_cells)
    }

    pub fn push_row_cancellable(
        &mut self,
        position: u32,
        key: &[Value],
        cancel: Option<&tokio_util::sync::CancellationToken>,
        visit_cells: impl FnOnce(&mut dyn FnMut(usize, &Value)),
    ) -> Result<(), ReplaceError> {
        if self.rows.len() >= MAX_REPLACE_SNAPSHOT_ROWS {
            return Err(ReplaceError::Budget);
        }
        if cancel.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
            return Err(ReplaceError::Cancelled);
        }
        let previous_bytes = self.bytes;
        let row_bytes = retained_key_bytes(key)
            .saturating_add(std::mem::size_of::<SnapshotRow>())
            .saturating_mul(2);
        if row_bytes > MAX_REPLACE_SNAPSHOT_BYTES.saturating_sub(self.bytes) {
            return Err(ReplaceError::Budget);
        }
        self.bytes += row_bytes;
        self.rows.push(SnapshotRow {
            position,
            key: key.to_vec(),
            cells: Vec::new(),
        });
        let mut over_budget = false;
        let mut cancelled = false;
        visit_cells(&mut |column, value| {
            if over_budget || cancelled {
                return;
            }
            if cancel.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
                cancelled = true;
                return;
            }
            let cell_bytes = retained_key_bytes(std::slice::from_ref(value))
                .saturating_add(std::mem::size_of::<(usize, Value)>())
                .saturating_mul(2);
            if cell_bytes > MAX_REPLACE_SNAPSHOT_BYTES.saturating_sub(self.bytes) {
                over_budget = true;
                return;
            }
            self.bytes += cell_bytes;
            if let Some(row) = self.rows.last_mut() {
                row.cells.push((column, value.clone()));
            }
        });
        if over_budget || cancelled {
            self.rows.pop();
            self.bytes = previous_bytes;
            return Err(if cancelled {
                ReplaceError::Cancelled
            } else {
                ReplaceError::Budget
            });
        }
        Ok(())
    }

    pub fn rows(&self) -> &[SnapshotRow] {
        &self.rows
    }
}
