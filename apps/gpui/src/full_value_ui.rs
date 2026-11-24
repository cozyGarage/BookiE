use bookie_gpui::full_value::{self, Kind};
use bookie_gpui::model::{Cell, CellText};
use bookie_gpui::theme::Theme;
use gpui::{Context, Div, Entity, Focusable, Window, div, prelude::*, px, rgb};
use gpui_kit::component::input::{Editor, EditorState};

use crate::{GridView, chrome::ghost};

pub(crate) struct FullValueEditor {
    row: u64,
    column: usize,
    kind: Kind,
    pub(crate) input: Entity<EditorState>,
    source: crate::Source,
    tab: bookie_gpui::shell::Tab,
    original_null: bool,
}

impl GridView {
    pub(crate) fn open_selected_full_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.form_open()
            || self.save.pending.is_some()
            || self.save.busy
            || self.compare.is_some()
            || !self.source.editable()
        {
            self.status = "This cell cannot be edited now".into();
            cx.notify();
            return;
        }
        let Some(cursor) = self.cell_cursor else {
            self.status = "Select a cell first".into();
            cx.notify();
            return;
        };
        if cursor.row >= self.visible_count() || cursor.column >= self.source.columns().len() {
            self.status = "The selected cell is no longer available".into();
            cx.notify();
            return;
        }
        let row = self.source_row(self.base() + cursor.row as u64);
        let cell = self.cell_at(row, cursor.column);
        if self.live.is_none() && !bookie_gpui::edit::is_editable(cursor.column, &cell) {
            self.status = "This cell cannot be edited".into();
            cx.notify();
            return;
        }
        self.open_full_value(row, cursor.column, &cell, window, cx);
    }
    pub(crate) fn open_full_value(
        &mut self,
        row: u64,
        column: usize,
        cell: &Cell,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let column_type = self
            .source
            .columns()
            .get(column)
            .map(|column| column.data_type.to_ascii_lowercase())
            .unwrap_or_default();
        let json_column = column_type == "json" || column_type == "jsonb";
        let kind = match cell {
            Cell::Null if json_column => Kind::JsonText,
            Cell::Null if column_type.contains("text") || column_type.contains("char") => Kind::Text,
            Cell::Text(_) if json_column => Kind::JsonFallbackText,
            Cell::Text(_) => Kind::Text,
            Cell::Json(_) => Kind::Json,
            Cell::JsonText(_) => Kind::JsonText,
            _ => {
                self.status = "This value cannot be opened in the full-value editor".into();
                cx.notify();
                return;
            }
        };
        if matches!(cell, Cell::Text(value) | Cell::JsonText(value) if value.len() > full_value::MAX_EDIT_BYTES) {
            self.status = "Value exceeds the 1 MiB edit limit".into();
            cx.notify();
            return;
        }
        if matches!(cell, Cell::Json(value) if !full_value::json_fits(value)) {
            self.status = "Value exceeds the 1 MiB edit limit".into();
            cx.notify();
            return;
        }
        let text = if matches!(cell, Cell::Null) {
            String::new()
        } else {
            cell.display()
        };
        if text.len() > full_value::MAX_EDIT_BYTES {
            self.status = "Value exceeds the 1 MiB edit limit".into();
            cx.notify();
            return;
        }
        let input = cx.new(|cx| EditorState::new(window, cx));
        cx.subscribe(&input, |_, _, event: &gpui_kit::component::input::InputEvent, cx| {
            if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        input.update(cx, |input, cx| input.set_value(text, window, cx));
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.full_value = Some(FullValueEditor {
            row,
            column,
            kind,
            input,
            source: self.source.clone(),
            tab: self.tabs.current().clone(),
            original_null: matches!(cell, Cell::Null),
        });
        cx.notify();
    }

    pub(crate) fn full_value_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let editor = self.full_value.as_ref()?;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .bg(rgb(theme.chrome))
                .child(div().child(if editor.kind == Kind::Text {
                    "Edit full value (maximum 1 MiB)"
                } else {
                    "Edit JSON (maximum 1 MiB; Format changes whitespace)"
                }))
                .child(div().child(format!(
                    "{} bytes / {} maximum",
                    editor.input.read(cx).text().len(),
                    full_value::MAX_EDIT_BYTES
                )))
                .child(
                    div()
                        .h(px(200.0))
                        .child(Editor::new(&editor.input).h(gpui::relative(1.0))),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .when(editor.original_null && editor.kind == Kind::Text, |bar| {
                            bar.child(ghost("empty-full-value", "Use empty text", theme).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.select_empty_full_value(cx);
                                },
                            )))
                        })
                        .when(editor.kind != Kind::Text, |bar| {
                            bar.child(ghost("format-full-value", "Format JSON", theme).on_click(cx.listener(
                                |this, _, window, cx| {
                                    let Some(editor) = this.full_value.as_ref() else { return };
                                    if editor.input.read(cx).text().len() > full_value::MAX_EDIT_BYTES {
                                        this.status = "Value exceeds the 1 MiB edit limit".into();
                                        cx.notify();
                                        return;
                                    }
                                    match bookie_app_services::json_format::pretty_json(
                                        editor.input.read(cx).value().as_ref(),
                                        full_value::MAX_EDIT_BYTES,
                                    ) {
                                        Ok(formatted) => editor
                                            .input
                                            .update(cx, |input, cx| input.set_value(formatted, window, cx)),
                                        Err(_) => this.status = "Could not format JSON within the edit limit".into(),
                                    }
                                    cx.notify();
                                },
                            )))
                        })
                        .child(ghost("apply-full-value", "Apply", theme).on_click(cx.listener(
                            |this, _, window, cx| {
                                this.apply_full_value(cx);
                                if this.full_value.is_none() {
                                    window.focus(&this.focus, cx);
                                }
                            },
                        )))
                        .child(ghost("cancel-full-value", "Cancel", theme).on_click(cx.listener(
                            |this, _, window, cx| {
                                this.cancel_full_value(cx);
                                window.focus(&this.focus, cx);
                            },
                        ))),
                ),
        )
    }

    pub(crate) fn cancel_full_value(&mut self, cx: &mut Context<Self>) {
        self.full_value = None;
        self.status = "Full-value edit cancelled".into();
        cx.notify();
    }

    pub(crate) fn select_empty_full_value(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.full_value.as_mut() {
            editor.original_null = false;
        }
        cx.notify();
    }

    pub(crate) fn apply_full_value(&mut self, cx: &mut Context<Self>) {
        let this = self;
        let Some(editor) = this.full_value.as_ref() else { return };
        if !std::sync::Arc::ptr_eq(&editor.source, &this.source)
            || editor.tab != *this.tabs.current()
            || this.save.pending.is_some()
            || this.save.busy
        {
            this.full_value = None;
            this.status = "The result changed; reopen the editor".into();
            cx.notify();
            return;
        }
        if editor.original_null && editor.input.read(cx).value().is_empty() {
            this.full_value = None;
            this.status = "NULL remains unchanged".into();
            cx.notify();
            return;
        }
        if editor.input.read(cx).text().len() > full_value::MAX_EDIT_BYTES {
            this.status = "Value exceeds the 1 MiB edit limit".into();
            cx.notify();
            return;
        }
        let text = editor.input.read(cx).value().to_string();
        match full_value::payload(editor.kind, &text) {
            Ok(value) => {
                let key = (editor.row, editor.column);
                if this.cell_at(key.0, key.1) == value {
                    this.full_value = None;
                    this.status = "Value unchanged".into();
                    cx.notify();
                    return;
                }
                this.record_edit(key, value);
                this.edits_changed();
                this.full_value = None;
                this.status = "Change staged; save to persist".into();
            }
            Err(error) => this.status = error.into(),
        }
        cx.notify();
    }
}
