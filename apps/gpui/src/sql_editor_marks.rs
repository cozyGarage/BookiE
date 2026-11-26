use std::ops::Range;

use bookie_app_services::completion_proposals::completion_plan;
use bookie_app_services::result_editability::grammar_for_driver;
use bookie_app_services::statement_plan::statement_range_in_plan;
use bookie_core::sql_syntax::script::ScriptPlan;
use bookie_core::sql_syntax::script::ScriptTokenKind;
use bookie_gpui::theme::size::GUTTER_MARKER_WIDTH;
use gpui::{Context, Entity, SharedString, px};
use gpui_component::input::{EditorState, GutterDecoration, InputEvent, RangeDecoration};

use crate::GridView;

const MARK_DOCUMENT_BYTES: usize = 256 * 1024;
const MAX_OCCURRENCES: usize = 100;

struct MarkSnapshot {
    revision: u64,
    driver: String,
    text: SharedString,
    plan: Option<ScriptPlan>,
}

impl MarkSnapshot {
    fn new(revision: u64, driver: String, text: SharedString) -> Self {
        let plan = grammar_for_driver(&driver).map(|grammar| completion_plan(&text, grammar, None));
        Self {
            revision,
            driver,
            text,
            plan,
        }
    }

    fn matches(&self, revision: u64, driver: &str) -> bool {
        self.revision == revision && self.driver == driver
    }
}

impl GridView {
    pub(crate) fn observe_sql_marks(input: &Entity<EditorState>, cx: &mut Context<Self>) {
        let (gutter, ranges) = input.update(cx, |editor, cx| {
            editor.set_gutter_decorations_width(px(GUTTER_MARKER_WIDTH), cx);
            (
                editor.create_gutter_decorations_collection(Vec::new(), cx),
                editor.create_range_decorations_collection(Vec::new(), cx),
            )
        });
        let edits = std::rc::Rc::new(std::cell::Cell::new(0_u64));
        let counted = edits.clone();
        cx.subscribe(input, move |_, _, event: &InputEvent, _| {
            if matches!(event, InputEvent::Change) {
                counted.set(counted.get().wrapping_add(1));
            }
        })
        .detach();
        let mut last = None;
        let mut snapshot = None;
        cx.observe(input, move |view, input, cx| {
            let editor = input.read(cx);
            let driver = view.live.as_ref().map_or("postgres", |session| &session.driver_id);
            let length = editor.text().len();
            let key = (
                edits.get(),
                length,
                editor.cursor(),
                editor.selected_range(),
                driver.to_owned(),
            );
            if last.as_ref() == Some(&key) {
                return;
            }
            let (marks, decorations) = if length > MARK_DOCUMENT_BYTES {
                snapshot = None;
                (Vec::new(), Vec::new())
            } else {
                if !snapshot
                    .as_ref()
                    .is_some_and(|snapshot: &MarkSnapshot| snapshot.matches(key.0, &key.4))
                {
                    snapshot = Some(MarkSnapshot::new(key.0, key.4.clone(), editor.value()));
                }
                match snapshot.as_ref() {
                    Some(snapshot) => {
                        let range = if key.3.is_empty() {
                            snapshot
                                .plan
                                .as_ref()
                                .and_then(|plan| statement_range_in_plan(&snapshot.text, plan, key.2))
                        } else {
                            crate::run_cursor_ui::run_source_range(&snapshot.text, driver, key.2, key.3.clone())
                        };
                        let marks = range.map(GutterDecoration::new).into_iter().collect();
                        let decorations = snapshot
                            .plan
                            .as_ref()
                            .map(|plan| matching_ranges(&snapshot.text, key.2, &key.3, plan))
                            .unwrap_or_default()
                            .into_iter()
                            .map(RangeDecoration::new)
                            .collect();
                        (marks, decorations)
                    }
                    None => (Vec::new(), Vec::new()),
                }
            };
            last = Some(key);
            gutter.set(marks, cx);
            ranges.set(decorations, cx);
            cx.notify();
        })
        .detach();
    }
}

fn matching_ranges(sql: &str, cursor: usize, selected: &Range<usize>, plan: &ScriptPlan) -> Vec<Range<usize>> {
    let tokens = || plan.tokens(0..sql.len());
    if !selected.is_empty() {
        let Some(word) = tokens()
            .find(|token| token.range == *selected && token.kind == ScriptTokenKind::Word)
            .and_then(|token| sql.get(token.range))
        else {
            return Vec::new();
        };
        return tokens()
            .filter(|token| {
                token.kind == ScriptTokenKind::Word
                    && token.range != *selected
                    && sql
                        .get(token.range.clone())
                        .is_some_and(|name| name.eq_ignore_ascii_case(word))
            })
            .take(MAX_OCCURRENCES)
            .map(|token| token.range)
            .collect();
    }
    let mut open = Vec::new();
    for token in tokens() {
        match token.kind {
            ScriptTokenKind::OpenParen => open.push(token.range),
            ScriptTokenKind::CloseParen => {
                if let Some(start) = open.pop()
                    && [start.start, start.end, token.range.start, token.range.end].contains(&cursor)
                {
                    return vec![start, token.range];
                }
            }
            _ => {}
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::sql_syntax::SqlGrammar;

    #[test]
    fn sql_marks_skip_matches_inside_literals_and_comments() {
        let sql = "select id, ID, 'id' -- id\nfrom items";
        let plan = completion_plan(sql, SqlGrammar::PostgreSql, None);
        assert_eq!(matching_ranges(sql, 9, &(7..9), &plan), vec![11..13]);
        let sql = "select ('(' /* ) */)";
        let plan = completion_plan(sql, SqlGrammar::PostgreSql, None);
        assert_eq!(
            matching_ranges(sql, sql.len(), &(sql.len()..sql.len()), &plan),
            vec![7..8, sql.len() - 1..sql.len()]
        );
    }

    #[test]
    fn mark_snapshots_are_reused_only_for_the_same_edit_and_driver() {
        let snapshot = MarkSnapshot::new(4, "postgres".into(), SharedString::from("SELECT 1"));

        assert!(snapshot.matches(4, "postgres"));
        assert!(!snapshot.matches(5, "postgres"));
        assert!(!snapshot.matches(4, "mysql"));
    }
}
