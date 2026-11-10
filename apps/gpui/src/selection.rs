use crate::model::Cell;

pub const MAX_COPY_ROWS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: u64,
    pub focus: u64,
}

impl Selection {
    pub fn after_click(current: Option<Selection>, row: u64, shift: bool) -> Selection {
        match current {
            Some(existing) if shift => Selection {
                anchor: existing.anchor,
                focus: row,
            },
            _ => Selection {
                anchor: row,
                focus: row,
            },
        }
    }

    pub fn first(self) -> u64 {
        self.anchor.min(self.focus)
    }

    pub fn last(self) -> u64 {
        self.anchor.max(self.focus)
    }

    pub fn count(self) -> u64 {
        self.last() - self.first() + 1
    }

    pub fn contains(self, row: u64) -> bool {
        (self.first()..=self.last()).contains(&row)
    }

    pub fn copyable(self) -> bool {
        self.count() <= MAX_COPY_ROWS
    }

    pub fn label(self) -> String {
        let count = self.count();
        let noun = if count == 1 { "row" } else { "rows" };
        let limit = if self.copyable() {
            String::new()
        } else {
            format!(" (copy is limited to {MAX_COPY_ROWS})")
        };
        format!("{count} {noun} selected{limit}")
    }
}

pub fn copy_tsv(rows: impl IntoIterator<Item = Vec<Cell>>, with_headers: bool) -> String {
    crate::core_bridge::render_tsv(rows, with_headers)
}
