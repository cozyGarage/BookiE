use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookmarkSet {
    lines: BTreeSet<u32>,
}

impl BookmarkSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_lines(lines: impl IntoIterator<Item = u32>) -> Self {
        Self {
            lines: lines.into_iter().collect(),
        }
    }

    pub fn toggle(&mut self, line: u32) -> bool {
        if self.lines.remove(&line) {
            false
        } else {
            self.lines.insert(line);
            true
        }
    }

    pub fn contains(&self, line: u32) -> bool {
        self.lines.contains(&line)
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn lines(&self) -> impl Iterator<Item = u32> + '_ {
        self.lines.iter().copied()
    }

    pub fn adjacent(&self, from: u32, forward: bool) -> Option<u32> {
        if forward {
            self.lines.range(from.saturating_add(1)..).next().copied()
        } else {
            self.lines.range(..from).next_back().copied()
        }
    }
}

#[cfg(test)]
#[path = "editor_bookmarks_tests.rs"]
mod tests;
