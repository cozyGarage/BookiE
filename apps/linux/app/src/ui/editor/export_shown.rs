use std::sync::Arc;

use bookie_core::QueryResult;
use relm4::adw::prelude::*;
use relm4::gtk::prelude::*;
use relm4::{adw, gtk};

use super::stream_view::visible_page_index;
use super::{SqlEditor, StatementOutcome, StatementOutcomeKind, StreamedResult};

pub(crate) fn pick_shown_export(
    outcomes: &[StatementOutcome],
    outcome_index: Option<usize>,
    set_index: Option<usize>,
    streamed_page: Option<Arc<QueryResult>>,
) -> Option<Arc<QueryResult>> {
    let index = outcome_index.unwrap_or(0);
    let outcome = outcomes.get(index)?;
    match &outcome.kind {
        StatementOutcomeKind::Rows(sets) => {
            let set = sets.get(set_index.unwrap_or(0))?;
            if set.columns.is_empty() {
                return None;
            }
            Some(set.clone())
        }
        StatementOutcomeKind::Streamed(_) => streamed_page.filter(|page| !page.columns.is_empty()),
        StatementOutcomeKind::Error(_) | StatementOutcomeKind::NotRun => None,
    }
}

fn parse_prefixed_index(name: &str, prefix: &str) -> Option<usize> {
    name.strip_prefix(prefix)?.parse().ok()
}

fn find_descendant<T: IsA<gtk::Widget>>(root: &gtk::Widget, matches: impl Fn(&gtk::Widget) -> bool) -> Option<T> {
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if matches(&widget)
            && let Ok(found) = widget.clone().downcast::<T>()
        {
            return Some(found);
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            pending.push(next);
        }
    }
    None
}

fn visible_stack_index(root: &gtk::Widget, prefix: &str) -> Option<usize> {
    let stack = find_descendant::<adw::ViewStack>(root, |widget| {
        widget
            .downcast_ref::<adw::ViewStack>()
            .and_then(adw::ViewStack::visible_child_name)
            .is_some_and(|name| parse_prefixed_index(name.as_str(), prefix).is_some())
    })?;
    parse_prefixed_index(stack.visible_child_name()?.as_str(), prefix)
}

fn streamed_page_for(holder: &gtk::Box, result: &StreamedResult) -> Option<Arc<QueryResult>> {
    let page_index = visible_page_index(holder, result.generation).or_else(|| result.page_indices.first().copied())?;
    result
        .resident_pages
        .get(page_index)
        .or_else(|| {
            result
                .pages
                .iter()
                .zip(result.page_indices.iter())
                .find(|(_, index)| **index == page_index)
                .and_then(|(page, _)| page.upgrade())
        })
        .or_else(|| result.pages.iter().find_map(std::sync::Weak::upgrade))
}

impl SqlEditor {
    pub(crate) fn export_shown_result(&self) -> Option<(Arc<QueryResult>, String)> {
        let outcomes = self.pinned.displayed();
        let root = self.results_holder.clone().upcast::<gtk::Widget>();
        let outcome_index = visible_stack_index(&root, "r");
        let set_index = visible_stack_index(&root, "set");
        let streamed_page = outcome_index
            .or(Some(0))
            .and_then(|index| outcomes.get(index))
            .and_then(|outcome| match &outcome.kind {
                StatementOutcomeKind::Streamed(result) => streamed_page_for(&self.results_holder, result),
                _ => None,
            });
        let result = pick_shown_export(&outcomes, outcome_index, set_index, streamed_page)?;
        Some((result, self.export_name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::editor::{StatementIdentity, StatementOutcome, StatementOutcomeKind};

    fn rows(sets: Vec<QueryResult>) -> StatementOutcome {
        StatementOutcome {
            identity: StatementIdentity::default(),
            sql_preview: String::new(),
            sql: String::new(),
            elapsed_ms: 0,
            kind: StatementOutcomeKind::Rows(sets.into_iter().map(Arc::new).collect()),
            pinned: false,
        }
    }

    fn column(name: &str) -> bookie_core::ColumnInfo {
        bookie_core::ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    fn query(columns: usize, row_count: usize) -> QueryResult {
        QueryResult {
            columns: (0..columns).map(|index| column(&format!("c{index}"))).collect(),
            rows: (0..row_count)
                .map(|_| {
                    (0..columns)
                        .map(|_| bookie_core::Value::Text("x".into()))
                        .collect::<Vec<_>>()
                        .into()
                })
                .collect::<Vec<_>>()
                .into(),
            truncated: false,
        }
    }

    fn streamed_outcome() -> StatementOutcome {
        StatementOutcome {
            identity: StatementIdentity::default(),
            sql_preview: String::new(),
            sql: String::new(),
            elapsed_ms: 0,
            kind: StatementOutcomeKind::Streamed(Box::new(StreamedResult {
                pages: Vec::new(),
                page_indices: Vec::new(),
                page_row_counts: Vec::new(),
                page_row_starts: Vec::new(),
                retained_rows: 0,
                columns: Vec::new(),
                total_pages: 0,
                total_rows: 0,
                terminal: super::super::stream_pages::StreamTerminal::Complete,
                export: None,
                generation: 1,
                spool: None,
                resident_pages: Default::default(),
            })),
            pinned: false,
        }
    }

    #[test]
    fn empty_and_error_outcomes_are_not_exportable() {
        let error = StatementOutcome {
            identity: StatementIdentity::default(),
            sql_preview: String::new(),
            sql: String::new(),
            elapsed_ms: 0,
            kind: StatementOutcomeKind::Error("boom".into()),
            pinned: false,
        };
        assert!(pick_shown_export(&[], None, None, None).is_none());
        assert!(pick_shown_export(&[error], None, None, None).is_none());
        assert!(pick_shown_export(&[rows(vec![query(0, 0)])], None, None, None).is_none());
        assert!(pick_shown_export(&[streamed_outcome()], None, None, None).is_none());
    }

    #[test]
    fn rows_use_the_selected_outcome_and_result_set() {
        let first = rows(vec![query(1, 1), query(2, 1)]);
        let second = rows(vec![query(3, 2)]);
        let picked = pick_shown_export(&[first, second], Some(1), Some(0), None).unwrap();
        assert_eq!(picked.columns.len(), 3);
        assert_eq!(picked.rows.len(), 2);
        let set = pick_shown_export(&[rows(vec![query(1, 1), query(4, 1)])], Some(0), Some(1), None).unwrap();
        assert_eq!(set.columns.len(), 4);
    }

    #[test]
    fn streamed_exports_only_a_loaded_page() {
        let page = Arc::new(query(2, 3));
        let picked = pick_shown_export(&[streamed_outcome()], None, None, Some(page.clone())).unwrap();
        assert!(Arc::ptr_eq(&picked, &page));
    }
}
