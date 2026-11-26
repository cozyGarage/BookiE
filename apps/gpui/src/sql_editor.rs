use std::rc::Rc;
use std::sync::Arc;

use bookie_app_services::completion::{SchemaIndex, referenced_tables};
use bookie_app_services::completion_proposals::candidates;
use bookie_app_services::result_editability::grammar_for_driver;
use bookie_core::sql_dialect::qualified_table;
use gpui::{App, Context, SharedString, Task, WeakEntity, Window};
use gpui_component::input::{CompletionProvider, EditorState, Rope, RopeExt};
use lsp_types::{CompletionContext, CompletionItem, CompletionResponse, CompletionTextEdit, Range, TextEdit};

use crate::{GridView, live::LiveSession};

const MAX_COMPLETION_BYTES: usize = 256 * 1024;
const MAX_COMPLETION_ITEMS: usize = 50;
const MAX_METADATA_TABLES: usize = 4;

struct SqlCompletion(WeakEntity<GridView>);

impl CompletionProvider for SqlCompletion {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        if text.len() > MAX_COMPLETION_BYTES {
            return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
        }
        let context = self.0.upgrade().and_then(|view| {
            let view = view.read(cx);
            Some((view.live.clone()?, view.schema_context.clone()))
        });
        let text = text.clone();
        cx.background_executor().spawn(async move {
            let items = context.map_or_else(Vec::new, |(session, schema)| {
                completion_items(&text, offset, &session, schema.as_deref())
            });
            Ok(CompletionResponse::Array(items))
        })
    }

    fn is_completion_trigger(&self, _: usize, inserted: &str, _: &mut App) -> bool {
        matches!(inserted, " " | ".")
    }
}

fn completion_items(
    text: &Rope,
    offset: usize,
    session: &Arc<LiveSession>,
    schema: Option<&str>,
) -> Vec<CompletionItem> {
    let Some(grammar) = grammar_for_driver(&session.driver_id) else {
        return Vec::new();
    };
    let sql = text.to_string();
    let mut index = SchemaIndex::default();
    index.grammar = Some(grammar);
    index.set_tables(
        session
            .tables
            .iter()
            .map(|table| qualified_table(&session.driver_id, table.schema.as_deref(), &table.name))
            .collect(),
    );
    for reference in referenced_tables(&sql, offset, Some(grammar))
        .into_iter()
        .take(MAX_METADATA_TABLES)
    {
        let Some((explicit_schema, name)) =
            bookie_app_services::completion_identifiers::split_table_reference(&reference, Some(grammar))
        else {
            continue;
        };
        let requested = explicit_schema.as_deref().or(schema);
        let mut matches = session.tables.iter().filter(|table| {
            table.name == name && requested.is_none_or(|schema| table.schema.as_deref() == Some(schema))
        });
        if let Some(table) = matches.next()
            && matches.next().is_none()
            && let Ok(columns) = session.completion_columns(table)
        {
            index.set_columns(&reference, columns.into_iter().map(|column| column.name).collect());
        }
    }
    let result = candidates(&sql, offset, &index);
    let range = Range::new(
        text.offset_to_position(result.range.start),
        text.offset_to_position(result.range.end),
    );
    result
        .values
        .into_iter()
        .take(MAX_COMPLETION_ITEMS)
        .map(|(label, new_text)| CompletionItem {
            label,
            text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(range, new_text))),
            ..CompletionItem::default()
        })
        .collect()
}

impl GridView {
    pub(crate) fn install_sql_completion(editor: &mut EditorState, view: WeakEntity<Self>) {
        editor.lsp_mut().completion_provider = Some(Rc::new(SqlCompletion(view)));
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
) -> gpui_component::menu::PopupMenu {
    use gpui_component::menu::PopupMenuItem;
    let snippets = bookie_app_services::snippets::SnippetStore::open().map_or_else(Vec::new, |store| store.list());
    menu = menu.separator();
    if snippets.is_empty() {
        return menu.item(PopupMenuItem::new("No saved SQL snippets").disabled(true));
    }
    for snippet in snippets {
        let owner = owner.clone();
        menu = menu.item(PopupMenuItem::new(snippet.name).on_click(move |_, window, cx| {
            let _ = owner.update(cx, |view, cx| view.insert_sql_snippet(&snippet.body, window, cx));
        }));
    }
    menu
}
