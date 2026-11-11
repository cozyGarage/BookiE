//! The grid's paging: which dataset rows are in view, and how the window moves.

pub const DEFAULT_PAGE_ROWS: u64 = 5_000;
pub const PAGE_SIZES: [u64; 4] = [500, 1_000, 5_000, 20_000];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pager {
    pub total: u64,
    pub page: u64,
    pub rows: u64,
    pub append: bool,
}

impl Pager {
    pub fn new(total: u64) -> Self {
        Pager {
            total,
            page: 0,
            rows: DEFAULT_PAGE_ROWS,
            append: false,
        }
    }

    pub fn pages(&self) -> u64 {
        self.total.div_ceil(self.rows).max(1)
    }

    pub fn first(&mut self) {
        if !self.append {
            self.page = 0;
        }
    }

    pub fn last(&mut self) {
        self.page = self.pages() - 1;
    }

    pub fn next(&mut self) {
        self.page = (self.page + 1).min(self.pages() - 1);
    }

    pub fn previous(&mut self) {
        if !self.append {
            self.page = self.page.saturating_sub(1);
        }
    }

    pub fn window(&self) -> (u64, u64) {
        if self.total == 0 {
            return (0, 0);
        }
        let end = ((self.page + 1) * self.rows).min(self.total);
        if self.append {
            (0, end)
        } else {
            (self.page * self.rows, end)
        }
    }

    pub fn range(&self) -> (u64, u64) {
        let (start, end) = self.window();
        if self.total == 0 { (0, 0) } else { (start + 1, end) }
    }

    pub fn contains_row(&mut self, row: u64) {
        self.page = (row / self.rows).min(self.pages() - 1);
    }

    pub fn with_total(self, total: u64) -> Self {
        Pager { total, page: 0, ..self }
    }

    pub fn resized(self, rows: u64) -> Self {
        let first_row = self.page * self.rows;
        let mut next = Pager { rows, page: 0, ..self };
        next.page = (first_row / rows).min(next.pages() - 1);
        next
    }
}

#[derive(Debug, Default)]
pub struct MatchCursor {
    pub matches: Vec<u64>,
    pub current: Option<usize>,
}

impl MatchCursor {
    pub fn set(&mut self, matches: Vec<u64>) {
        self.matches = matches;
        self.current = None;
    }

    pub fn step(&mut self, forward: bool) -> Option<u64> {
        let count = self.matches.len();
        if count == 0 {
            return None;
        }
        let next = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
        };
        self.current = Some(next);
        self.matches.get(next).copied()
    }

    pub fn status(&self, capped: bool) -> String {
        let count = self.matches.len();
        if count == 0 {
            return "No match".into();
        }
        let shown = self.current.map_or(0, |index| index + 1);
        let more = if capped { "+" } else { "" };
        format!("Match {shown} of {count}{more}")
    }
}
