use super::*;

impl GridView {
    pub(crate) fn run_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.dataset_count() == 0 || self.find_text(cx).is_empty() {
            return;
        }
        if !self.search.cursor.matches.is_empty() && self.search.cursor.status(false) != "No match" {
            self.step_match(forward);
            return;
        }
        let (source, query) = (self.source.clone(), self.find_text(cx));
        self.status = "Searching…".into();
        self.begin_background(
            Work::Scan,
            cx,
            move |cancel| source.find(&query, MAX_FIND_MATCHES, &cancel),
            move |this, found| match found {
                Ok(found) => {
                    this.search.capped = found.capped;
                    this.search.cursor.set(found.matches);
                    this.step_match(forward);
                }
                Err(message) => this.status = message.into(),
            },
        );
    }

    pub(crate) fn step_match(&mut self, forward: bool) {
        if let Some(row) = self.search.cursor.step(forward) {
            let Some(at) = self.dataset_index_of(row) else {
                self.status = "Match is outside the filtered rows".into();
                return;
            };
            self.selection = Some(Selection::after_click(None, at, false));
            if !self.continuous {
                self.pager.contains_row(at);
            }
            let local = at.saturating_sub(self.base()) as usize;
            self.scroll.scroll_to_item_strict(local, ScrollStrategy::Center);
        }
        self.status = self.search.cursor.status(self.search.capped);
    }

    pub(crate) fn find_text(&self, cx: &App) -> String {
        self.search.find.read(cx).content().to_string()
    }

    pub(crate) fn show_matching(&mut self, cx: &App) {
        if self.search.cursor.matches.is_empty() {
            return;
        }
        let kept = self.search.cursor.matches.iter().map(|row| *row as u32).collect();
        let label = format!("FILTER {}", self.find_text(cx));
        self.set_mode(Mode::Filtered {
            kept: std::sync::Arc::new(kept),
            label,
        });
        self.status = format!("Showing {} matching rows", thousands(self.dataset_count()));
    }

    pub(crate) fn apply_filter(&mut self, cx: &mut Context<Self>) {
        if matches!(self.tabs.current(), Tab::Query) && self.runs.is_empty() {
            return;
        }
        let predicate = match parse_predicate(self.search.filter.read(cx).content(), self.source.columns()) {
            Ok(predicate) => predicate,
            Err(message) => {
                self.status = message.into();
                return;
            }
        };
        let source = self.source.clone();
        let label = format!("WHERE {}", self.search.filter.read(cx).content());
        self.status = "Filtering…".into();
        self.begin_background(
            Work::Scan,
            cx,
            move |cancel| source.filter(&predicate, &cancel),
            move |this, kept| {
                let kept = match kept {
                    Ok(kept) => kept,
                    Err(message) => {
                        this.status = message.into();
                        return;
                    }
                };
                this.set_mode(Mode::Filtered {
                    kept: std::sync::Arc::new(kept),
                    label,
                });
                this.status = format!("Showing {} matching rows", thousands(this.dataset_count()));
            },
        );
    }

    pub(crate) fn clear_filter(&mut self) {
        self.invalidate_scans();
        self.search.cursor = MatchCursor::default();
        self.set_mode(Mode::All);
        self.status.clear();
    }

    pub(crate) fn compare(&mut self, keyed: bool, cx: &mut Context<Self>) {
        if matches!(self.tabs.current(), Tab::Query) && self.runs.is_empty() {
            return;
        }
        let source = self.source.clone();
        self.status = "Comparing…".into();
        self.begin_background(
            Work::Scan,
            cx,
            move |cancel| source.compare(keyed, &cancel),
            |this, result| match result {
                Ok(diff) => {
                    this.status.clear();
                    this.selection = None;
                    this.compare = Some(std::sync::Arc::new(diff));
                    this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                }
                Err(message) => this.status = message.into(),
            },
        );
    }
}
