use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bookie_app_services::completion_proposals::completion_plan;
use bookie_app_services::result_editability::grammar_for_driver;
use bookie_app_services::statement_plan::statement_range_in_plan;
use bookie_core::sql_syntax::script::ScriptPlan;
use bookie_core::sql_syntax::script::ScriptTokenKind;
use bookie_gpui::theme::size::GUTTER_MARKER_WIDTH;
use gpui::{App, AsyncApp, Context, Entity, WeakEntity, px};
use gpui_component::input::{
    EditorState, GutterDecoration, GutterDecorationCollection, InputEvent, RangeDecoration, RangeDecorationCollection,
};

use crate::GridView;

const MARK_DOCUMENT_BYTES: usize = 256 * 1024;
const MAX_OCCURRENCES: usize = 100;
const EDIT_DEBOUNCE: Duration = Duration::from_millis(80);
const CURSOR_DEBOUNCE: Duration = Duration::from_millis(16);

#[derive(Default)]
pub(crate) struct SqlMarkRevision {
    value: Cell<u64>,
    #[cfg(test)]
    plan_builds: Cell<usize>,
}

impl SqlMarkRevision {
    fn get(&self) -> u64 {
        self.value.get()
    }

    pub(crate) fn advance(&self) {
        self.value.set(self.get().wrapping_add(1));
    }

    #[cfg(test)]
    pub(crate) fn plan_builds(&self) -> usize {
        self.plan_builds.get()
    }

    #[cfg(test)]
    fn note_plan_build(&self) {
        self.plan_builds.set(self.plan_builds.get() + 1);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MarkKey {
    revision: u64,
    length: usize,
    cursor: usize,
    selected: Range<usize>,
    driver: String,
}

struct MarkSnapshot {
    revision: u64,
    driver: String,
    text: String,
    plan: Option<ScriptPlan>,
}

impl MarkSnapshot {
    fn new(revision: u64, driver: String, text: String) -> Self {
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

#[derive(Clone)]
struct MarkState {
    revision: Rc<SqlMarkRevision>,
    snapshot: Rc<RefCell<Option<Arc<MarkSnapshot>>>>,
    last_applied: Rc<RefCell<Option<MarkKey>>>,
    pending: Rc<Cell<bool>>,
    decorations_cleared: Rc<Cell<bool>>,
    gutter: GutterDecorationCollection,
    ranges: RangeDecorationCollection,
}

fn mark_key(view: &GridView, cx: &App, revision: u64) -> MarkKey {
    let editor = view.sql_input.read(cx);
    MarkKey {
        revision,
        length: editor.text().len(),
        cursor: editor.cursor(),
        selected: editor.selected_range(),
        driver: view
            .live
            .as_ref()
            .map_or("postgres", |session| &session.driver_id)
            .to_owned(),
    }
}

fn mark_ranges(snapshot: &MarkSnapshot, key: &MarkKey) -> (Option<Range<usize>>, Vec<Range<usize>>) {
    let statement = if key.selected.is_empty() {
        snapshot
            .plan
            .as_ref()
            .and_then(|plan| statement_range_in_plan(&snapshot.text, plan, key.cursor))
    } else {
        crate::run_cursor_ui::run_source_range(&snapshot.text, &key.driver, key.cursor, key.selected.clone())
    };
    let ranges = snapshot
        .plan
        .as_ref()
        .map(|plan| matching_ranges(&snapshot.text, key.cursor, &key.selected, plan))
        .unwrap_or_default();
    (statement, ranges)
}

impl GridView {
    pub(crate) fn observe_sql_marks(input: &Entity<EditorState>, cx: &mut Context<Self>) -> Rc<SqlMarkRevision> {
        let (gutter, ranges) = input.update(cx, |editor, cx| {
            editor.set_gutter_decorations_width(px(GUTTER_MARKER_WIDTH), cx);
            (
                editor.create_gutter_decorations_collection(Vec::new(), cx),
                editor.create_range_decorations_collection(Vec::new(), cx),
            )
        });
        let revision = Rc::new(SqlMarkRevision::default());
        let edited_revision = revision.clone();
        cx.subscribe(input, move |_, _, event: &InputEvent, _| {
            if matches!(event, InputEvent::Change) {
                edited_revision.advance();
            }
        })
        .detach();
        let state = MarkState {
            revision,
            snapshot: Rc::new(RefCell::new(None)),
            last_applied: Rc::new(RefCell::new(None)),
            pending: Rc::new(Cell::new(false)),
            decorations_cleared: Rc::new(Cell::new(false)),
            gutter,
            ranges,
        };
        let mark_revision = state.revision.clone();
        cx.observe(input, move |view, _, cx| observe_marks(view, cx, state.clone()))
            .detach();
        mark_revision
    }
}

fn observe_marks(view: &GridView, cx: &mut Context<GridView>, state: MarkState) {
    let key = mark_key(view, cx, state.revision.get());
    if key.length == 0 || key.length > MARK_DOCUMENT_BYTES {
        invalidate_marks(&state, key, cx);
        return;
    }
    if state.last_applied.borrow().as_ref() == Some(&key) {
        return;
    }
    let has_snapshot = state
        .snapshot
        .borrow()
        .as_ref()
        .is_some_and(|cached| cached.matches(key.revision, &key.driver));
    if !has_snapshot {
        clear_decorations(&state, cx);
    }
    if !state.pending.replace(true) {
        let worker_state = state.clone();
        cx.spawn(async move |this, cx| run_mark_worker(this, cx.clone(), worker_state).await)
            .detach();
    }
}

fn clear_decorations(state: &MarkState, cx: &mut Context<GridView>) {
    if !state.decorations_cleared.replace(true) {
        state.gutter.set(Vec::new(), cx);
        state.ranges.set(Vec::new(), cx);
        cx.notify();
    }
}

fn invalidate_marks(state: &MarkState, key: MarkKey, cx: &mut Context<GridView>) {
    state.snapshot.borrow_mut().take();
    *state.last_applied.borrow_mut() = Some(key);
    clear_decorations(state, cx);
}

async fn run_mark_worker(this: WeakEntity<GridView>, mut cx: AsyncApp, state: MarkState) {
    loop {
        let Some(before) = this
            .update(&mut cx, |view, cx| mark_key(view, cx, state.revision.get()))
            .ok()
        else {
            state.pending.set(false);
            return;
        };
        let cached = state
            .snapshot
            .borrow()
            .as_ref()
            .filter(|cached| cached.matches(before.revision, &before.driver))
            .cloned();
        let delay = if cached.is_some() {
            CURSOR_DEBOUNCE
        } else {
            EDIT_DEBOUNCE
        };
        cx.background_executor().timer(delay).await;

        let Some(key) = this
            .update(&mut cx, |view, cx| mark_key(view, cx, state.revision.get()))
            .ok()
        else {
            state.pending.set(false);
            return;
        };
        if key != before {
            continue;
        }
        if key.length == 0 || key.length > MARK_DOCUMENT_BYTES {
            let _ = this.update(&mut cx, |_, cx| invalidate_marks(&state, key, cx));
            state.pending.set(false);
            return;
        }

        let cached = state
            .snapshot
            .borrow()
            .as_ref()
            .filter(|cached| cached.matches(key.revision, &key.driver))
            .cloned();
        let (text_snapshot, rope) = if let Some(cached) = cached {
            (Some(cached), None)
        } else {
            let Some(rope) = this
                .update(&mut cx, |view, cx| view.sql_input.read(cx).text().clone())
                .ok()
            else {
                state.pending.set(false);
                return;
            };
            (None, Some(rope))
        };
        let target = key.clone();
        #[cfg(test)]
        let builds_plan = text_snapshot.is_none();
        #[cfg(test)]
        if builds_plan {
            state.revision.note_plan_build();
        }
        let built = cx
            .background_executor()
            .spawn(async move {
                let snapshot = match text_snapshot {
                    Some(snapshot) => snapshot,
                    None => {
                        let rope = rope?;
                        Arc::new(MarkSnapshot::new(
                            target.revision,
                            target.driver.clone(),
                            rope.to_string(),
                        ))
                    }
                };
                let marks = mark_ranges(&snapshot, &target);
                Some((snapshot, marks))
            })
            .await;

        let Some((text_snapshot, (statement, matches))) = built else {
            state.pending.set(false);
            return;
        };
        let applied = this
            .update(&mut cx, |view, cx| {
                if mark_key(view, cx, state.revision.get()) != key {
                    return false;
                }
                *state.snapshot.borrow_mut() = Some(text_snapshot);
                *state.last_applied.borrow_mut() = Some(key);
                state.decorations_cleared.set(false);
                state
                    .gutter
                    .set(statement.map(GutterDecoration::new).into_iter().collect(), cx);
                state
                    .ranges
                    .set(matches.into_iter().map(RangeDecoration::new).collect(), cx);
                cx.notify();
                true
            })
            .unwrap_or(false);
        if applied {
            state.pending.set(false);
            return;
        }
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
    use gpui::TestAppContext;

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
    fn mark_snapshot_is_reused_by_document_revision_and_driver() {
        let snapshot = MarkSnapshot::new(4, "postgres".into(), "SELECT 1".into());

        assert!(snapshot.matches(4, "postgres"));
        assert!(!snapshot.matches(5, "postgres"));
        assert!(!snapshot.matches(4, "mysql"));
    }

    #[gpui::test]
    fn rapid_large_sql_replacements_build_one_mark_plan_and_cursor_moves_reuse_it(cx: &mut TestAppContext) {
        let (view, cx) = crate::ui_tests::open(cx);
        let comment = "x".repeat(200 * 1024);
        for (index, statement) in ["SELECT 1", "SELECT 2", "SELECT 3"].into_iter().enumerate() {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.set_sql_text(format!("{statement}; -- {comment}\n"), window, cx);
                });
            });
            cx.run_until_parked();
            if index < 2 {
                cx.executor().advance_clock(Duration::from_millis(25));
                cx.run_until_parked();
            }
        }
        cx.executor().advance_clock(EDIT_DEBOUNCE);
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.sql_mark_revision.plan_builds(),
                0,
                "stale edits restart the debounce"
            );
        });
        cx.executor().advance_clock(EDIT_DEBOUNCE);
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.sql_mark_revision.plan_builds(), 1);
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.sql_input.update(cx, |editor, cx| {
                    editor.set_cursor_position(lsp_types::Position::new(0, 4), window, cx)
                });
            });
        });
        cx.executor().advance_clock(CURSOR_DEBOUNCE);
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.sql_mark_revision.plan_builds(),
                1,
                "cursor movement reuses the text plan"
            );
        });
    }
}
