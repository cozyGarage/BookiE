use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, Weak};

use bookie_app_services::completion::{SchemaIndex, referenced_tables};
use bookie_app_services::completion_proposals::{Candidate, CandidateKind, ranked_candidates};
use bookie_app_services::result_editability::grammar_for_driver;
use bookie_core::sql_dialect::qualified_table;
use gpui::{App, Context, SharedString, Task, WeakEntity, Window};
use gpui_component::input::{CompletionProvider, EditorState, InputContent, Rope, RopeExt};
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, Range, TextEdit,
};

use bookie_gpui::live::LiveSession;

use crate::GridView;

const MAX_COMPLETION_BYTES: usize = 256 * 1024;
const MAX_COMPLETION_ITEMS: usize = 50;
const MAX_METADATA_TABLES: usize = 4;

#[derive(Default)]
struct CompletionIndex {
    session: Weak<LiveSession>,
    index: SchemaIndex,
}

impl CompletionIndex {
    fn prepare(&mut self, session: &Arc<LiveSession>, grammar: bookie_core::sql_syntax::SqlGrammar) {
        let identity = Arc::downgrade(session);
        if !self.session.ptr_eq(&identity) {
            self.session = identity;
            self.index.grammar = Some(grammar);
            self.index.set_tables(
                session
                    .tables
                    .iter()
                    .map(|table| qualified_table(&session.driver_id, table.schema.as_deref(), &table.name))
                    .collect(),
            );
        }
        self.index.invalidate_columns();
    }
}

struct SqlCompletion(WeakEntity<GridView>, Arc<Mutex<CompletionIndex>>);

impl CompletionProvider for SqlCompletion {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        trigger: CompletionContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        if text.len() > MAX_COMPLETION_BYTES {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        }
        if trigger.trigger_kind == lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER && !auto_suggest(text, offset) {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        }
        let text = text.clone();
        let view = self.0.clone();
        let cache = self.1.clone();
        cx.spawn(async move |cx| {
            let context = view
                .read_with(cx, |view, _| Some((view.live.clone()?, view.schema_context.clone())))
                .ok()
                .flatten();
            let Some((session, schema)) = context else {
                return Ok(CompletionResponse::Array(Vec::new()));
            };
            let current_session = session.clone();
            let current_schema = schema.clone();
            let items = cx
                .background_executor()
                .spawn(async move { completion_items(&text, offset, &session, schema.as_deref(), &cache) })
                .await;
            let still_current = view
                .read_with(cx, |view, _| {
                    view.live
                        .as_ref()
                        .is_some_and(|session| Arc::ptr_eq(session, &current_session))
                        && view.schema_context == current_schema
                })
                .unwrap_or(false);
            Ok(CompletionResponse::Array(if still_current {
                items
            } else {
                Vec::new()
            }))
        })
    }

    fn is_completion_trigger(&self, _: usize, inserted: &str, _: &mut App) -> bool {
        matches!(inserted, " " | ".")
    }
}

fn auto_suggest(text: &Rope, offset: usize) -> bool {
    let point = text.offset_to_position(offset);
    let start = text.position_to_offset(&lsp_types::Position::new(point.line, 0));
    let line = text.slice(start..offset).to_string();
    if line.ends_with('.') {
        return true;
    }
    line.split_whitespace().next_back().is_some_and(|word| {
        [
            "FROM", "JOIN", "INTO", "UPDATE", "SELECT", "WHERE", "ON", "AND", "OR", "BY",
        ]
        .iter()
        .any(|keyword| word.eq_ignore_ascii_case(keyword))
    })
}

fn completion_items(
    text: &Rope,
    offset: usize,
    session: &Arc<LiveSession>,
    schema: Option<&str>,
    cache: &Mutex<CompletionIndex>,
) -> Vec<CompletionItem> {
    let Some(grammar) = grammar_for_driver(&session.driver_id) else {
        return Vec::new();
    };
    let sql = text.to_string();
    let columns = referenced_tables(&sql, offset, Some(grammar))
        .into_iter()
        .take(MAX_METADATA_TABLES)
        .filter_map(|reference| {
            let (explicit_schema, name) =
                bookie_app_services::completion_identifiers::split_table_reference(&reference, Some(grammar))?;
            let requested = explicit_schema.as_deref().or(schema);
            let mut matches = session.tables.iter().filter(|table| {
                table.name == name && requested.is_none_or(|schema| table.schema.as_deref() == Some(schema))
            });
            let table = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            Some((reference, session.completion_columns(table).ok()?))
        })
        .collect::<Vec<_>>();
    let types: HashMap<String, String> = columns
        .iter()
        .flat_map(|(_, columns)| columns.iter())
        .map(|column| (column.name.clone(), column.data_type.clone()))
        .collect();
    let Ok(mut cache) = cache.lock() else { return Vec::new() };
    cache.prepare(session, grammar);
    for (reference, columns) in columns {
        cache
            .index
            .set_columns(&reference, columns.into_iter().map(|column| column.name).collect());
    }
    let result = ranked_candidates(&sql, offset, &cache.index);
    drop(cache);
    let range = Range::new(
        text.offset_to_position(result.range.start),
        text.offset_to_position(result.range.end),
    );
    result
        .items
        .into_iter()
        .take(MAX_COMPLETION_ITEMS)
        .enumerate()
        .map(|(rank, candidate)| completion_item(rank, candidate, range, &types))
        .collect()
}

fn completion_item(rank: usize, candidate: Candidate, range: Range, types: &HashMap<String, String>) -> CompletionItem {
    let kind = match candidate.kind {
        CandidateKind::Column => CompletionItemKind::FIELD,
        CandidateKind::Local => CompletionItemKind::VARIABLE,
        CandidateKind::Function => CompletionItemKind::FUNCTION,
        CandidateKind::Table => CompletionItemKind::CLASS,
        CandidateKind::Keyword => CompletionItemKind::KEYWORD,
    };
    let data_type = (candidate.kind == CandidateKind::Column)
        .then(|| types.get(&candidate.label))
        .flatten();
    let detail = match (data_type, candidate.detail) {
        (Some(data_type), Some(table)) => Some(format!("{data_type} · {table}")),
        (_, table) => table,
    };
    CompletionItem {
        label: candidate.label,
        kind: Some(kind),
        detail,
        sort_text: Some(format!("{rank:04}")),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(range, candidate.insert))),
        ..CompletionItem::default()
    }
}

impl GridView {
    pub(crate) fn load_sql_snippets(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let snippets = cx
                .background_executor()
                .spawn(async move { read_saved_snippets() })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.sql_snippets = Some(snippets);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn set_sql_text(&mut self, text: impl Into<InputContent>, window: &mut Window, cx: &mut Context<Self>) {
        self.sql_input
            .update(cx, |editor, cx| editor.set_value(text, window, cx));
        self.sql_mark_revision.advance();
    }

    pub(crate) fn install_sql_completion(editor: &mut EditorState, view: WeakEntity<Self>) {
        editor.lsp_mut().completion_provider = Some(Rc::new(SqlCompletion(view, Arc::default())));
    }

    pub(crate) fn clear_sql_diagnostics(editor: &mut EditorState, cx: &mut Context<EditorState>) {
        if let Some(diagnostics) = editor.diagnostics_mut() {
            diagnostics.clear();
        }
        cx.notify();
    }

    pub(crate) fn show_sql_diagnostic(&self, message: String, cx: &mut Context<Self>) {
        let Some((sql, position, _, source)) = &self.panel.error_location else {
            return;
        };
        self.sql_input.update(cx, |editor, cx| {
            let Some((_, bytes)) =
                crate::run_cursor_ui::mapped_error_location(editor.value().as_ref(), sql, *position, source.as_ref())
            else {
                return;
            };
            let start = editor.text().offset_to_position(bytes.start);
            let end = editor.text().offset_to_position(bytes.end);
            if let Some(diagnostics) = editor.diagnostics_mut() {
                diagnostics.clear();
                diagnostics.push(lsp_types::Diagnostic {
                    range: Range::new(start, end),
                    severity: Some(lsp_types::DiagnosticSeverity::ERROR),
                    source: Some("Database".to_owned()),
                    message,
                    ..Default::default()
                });
                cx.notify();
            }
        });
    }

    pub(crate) fn insert_sql_snippet(&mut self, body: &str, window: &mut Window, cx: &mut Context<Self>) {
        let expansion = bookie_app_services::snippets::expand(body);
        self.sql_input.update(cx, |editor, cx| {
            let start = editor.selected_range().start;
            editor.replace(SharedString::from(expansion.text), window, cx);
            editor.set_selected_range(start + expansion.cursor..start + expansion.cursor, cx);
            editor.focus(window, cx);
        });
    }
}

pub(crate) fn snippet_menu(
    mut menu: gpui_component::menu::PopupMenu,
    owner: &WeakEntity<GridView>,
    snippets: Option<Vec<bookie_app_services::snippets::Snippet>>,
) -> gpui_component::menu::PopupMenu {
    use bookie_app_services::snippets::Snippet;
    use gpui_component::menu::PopupMenuItem;
    let loading = snippets.is_none();
    let snippets = snippets.unwrap_or_default();
    menu = menu.separator();
    if loading {
        menu = menu.item(PopupMenuItem::new("Loading saved snippets…").disabled(true));
    }
    let examples = [
        ("SELECT template", "SELECT $0\nFROM "),
        (
            "Common table expression",
            "WITH query AS (\n    $0\n)\nSELECT * FROM query;",
        ),
    ]
    .into_iter()
    .map(|(name, body)| Snippet {
        name: name.into(),
        body: body.into(),
    });
    for snippet in examples.chain(snippets) {
        let owner = owner.clone();
        menu = menu.item(PopupMenuItem::new(snippet.name).on_click(move |_, window, cx| {
            let _ = owner.update(cx, |view, cx| view.insert_sql_snippet(&snippet.body, window, cx));
        }));
    }
    menu
}

fn read_saved_snippets() -> Vec<bookie_app_services::snippets::Snippet> {
    bookie_app_services::snippets::SnippetStore::open().map_or_else(Vec::new, |store| store.list())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn a_column_completion_carries_its_kind_type_owner_and_rank() {
        let range = Range::default();
        let types = HashMap::from([("total".to_string(), "REAL".to_string())]);
        let column = completion_item(
            0,
            Candidate {
                label: "total".into(),
                insert: "\"total\"".into(),
                kind: CandidateKind::Column,
                detail: Some("orders".into()),
            },
            range,
            &types,
        );
        assert_eq!(column.kind, Some(CompletionItemKind::FIELD));
        assert_eq!(column.detail.as_deref(), Some("REAL · orders"));
        let function = completion_item(
            1,
            Candidate {
                label: "COUNT".into(),
                insert: "COUNT".into(),
                kind: CandidateKind::Function,
                detail: None,
            },
            range,
            &types,
        );
        assert_eq!(function.kind, Some(CompletionItemKind::FUNCTION));
        let keyword = completion_item(
            2,
            Candidate {
                label: "SELECT".into(),
                insert: "SELECT".into(),
                kind: CandidateKind::Keyword,
                detail: None,
            },
            range,
            &types,
        );
        assert_eq!(keyword.kind, Some(CompletionItemKind::KEYWORD));
        assert!(column.sort_text < function.sort_text);
        assert!(function.sort_text < keyword.sort_text);
    }

    #[test]
    fn automatic_completion_is_limited_to_sql_boundaries() {
        for (sql, expected) in [
            ("SELECT 1 ", false),
            ("select x FROM ", true),
            ("o.", true),
            ("雪 JOIN ", true),
        ] {
            assert_eq!(auto_suggest(&Rope::from(sql), sql.len()), expected, "{sql}");
        }
    }

    #[gpui::test]
    fn saved_snippets_are_loaded_into_the_window(cx: &mut TestAppContext) {
        let (view, cx) = crate::ui_tests::open(cx);
        let store = bookie_app_services::snippets::SnippetStore::open();
        assert!(store.is_some(), "the isolated snippet path should exist");
        let Some(store) = store else { return };
        let name = format!("background-load-{}", std::process::id());
        assert!(store.save(&name, "SELECT $0").is_ok());
        assert!(store.flush().is_ok());

        cx.update(|_, cx| view.update(cx, |_, cx| GridView::load_sql_snippets(cx)));
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(
                view.sql_snippets
                    .as_ref()
                    .is_some_and(|snippets| snippets.iter().any(|snippet| snippet.name == name))
            );
        });
    }
}
