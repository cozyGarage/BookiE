use super::*;

#[derive(Clone)]
pub(crate) struct EditOp {
    pub key: (u64, usize),
    pub before: Option<Cell>,
    pub after: Option<Cell>,
    weight: usize,
}

const MAX_UNDO_OPS: usize = 200;
const MAX_UNDO_BYTES: usize = 4 * 1024 * 1024;
const SMALL_CELL_BYTES: usize = 64;

fn cell_weight(cell: &Cell) -> usize {
    match cell {
        Cell::Text(text) | Cell::JsonText(text) => text.len(),
        Cell::Bytes(bytes) => bytes.len(),
        Cell::Json(value) => value.to_string().len(),
        _ => SMALL_CELL_BYTES,
    }
}

impl GridView {
    pub(crate) fn record_edit(&mut self, key: (u64, usize), after: Cell) {
        let before = self.cell_edit.edits.insert(key, after.clone());
        let weight = before.as_ref().map_or(0, cell_weight) + cell_weight(&after);
        self.cell_edit.undo.push(EditOp {
            key,
            before,
            after: Some(after),
            weight,
        });
        self.cell_edit.redo.clear();
        self.trim_undo_history();
    }

    fn trim_undo_history(&mut self) {
        let mut bytes: usize = self.cell_edit.undo.iter().map(|op| op.weight).sum();
        while self.cell_edit.undo.len() > 1 && (self.cell_edit.undo.len() > MAX_UNDO_OPS || bytes > MAX_UNDO_BYTES) {
            bytes -= self.cell_edit.undo.remove(0).weight;
        }
    }

    pub(crate) fn reset_edit_history(&mut self) {
        self.cell_edit.undo.clear();
        self.cell_edit.redo.clear();
    }

    pub(crate) fn can_undo_edit(&self) -> bool {
        !self.cell_edit.undo.is_empty() && !self.save.busy && self.save.pending.is_none()
    }

    pub(crate) fn can_redo_edit(&self) -> bool {
        !self.cell_edit.redo.is_empty() && !self.save.busy && self.save.pending.is_none()
    }

    pub(crate) fn undo_edit(&mut self, cx: &mut Context<Self>) {
        if self.save.busy || self.save.pending.is_some() {
            self.status = "Save or cancel the review before undoing".into();
            return;
        }
        let Some(op) = self.cell_edit.undo.pop() else {
            self.status = "Nothing to undo".into();
            return;
        };
        self.set_staged(op.key, op.before.clone());
        self.cell_edit.redo.push(op);
        self.edits_changed();
        self.status = "Undid an edit".into();
        cx.notify();
    }

    pub(crate) fn redo_edit(&mut self, cx: &mut Context<Self>) {
        if self.save.busy || self.save.pending.is_some() {
            self.status = "Save or cancel the review before redoing".into();
            return;
        }
        let Some(op) = self.cell_edit.redo.pop() else {
            self.status = "Nothing to redo".into();
            return;
        };
        self.set_staged(op.key, op.after.clone());
        self.cell_edit.undo.push(op);
        self.edits_changed();
        self.status = "Redid an edit".into();
        cx.notify();
    }

    fn set_staged(&mut self, key: (u64, usize), value: Option<Cell>) {
        match value {
            Some(cell) => {
                self.cell_edit.edits.insert(key, cell);
            }
            None => {
                self.cell_edit.edits.remove(&key);
            }
        }
    }
}
