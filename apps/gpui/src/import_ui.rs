use std::path::PathBuf;

use super::*;
use crate::connection_form::{field, labelled_row};
use crate::save_ui::{ReviewLabels, SaveRequest};
use crate::surfaces::{ButtonKind, button, segment, segmented};

pub(crate) const IMPORT_ROW_CAP: usize = 1_000;

#[derive(Default)]
pub(crate) struct ImportState {
    pub open: bool,
    pub input: Option<Entity<TextInput>>,
    pub delimiter: Option<bookie_core::export::CsvDelimiter>,
    pub null_input: Option<Entity<TextInput>>,
}

impl GridView {
    pub(crate) fn form_open(&self) -> bool {
        self.insert.open || self.import.open || self.structure.open
    }

    pub(crate) fn open_import_bar(&mut self, cx: &mut Context<Self>) {
        if self.compare.is_some() {
            self.status = "Leave compare mode before importing".into();
            return;
        }
        if !self.cell_edit.edits.is_empty() {
            self.status = "Save or discard the pending edits before importing".into();
            return;
        }
        if self.form_open() || self.insert.loading {
            return;
        }
        if self.save_session().is_none() {
            return;
        }
        let input = self.import.input.get_or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new("Path of a .csv or .xlsx file", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Submit => this.review_import(cx),
                InputEvent::Cancel => this.cancel_import_bar(cx),
                InputEvent::Changed => {}
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| input.set_content("", cx));
        if self.import.null_input.is_none() {
            self.import.null_input = Some(cx.new(|cx| TextInput::new("Leave empty: an empty field is NULL", cx)));
        }
        self.import.open = true;
        self.status = "Type the path of the file to import, then review it".into();
        cx.notify();
    }

    pub(crate) fn close_import_bar(&mut self) {
        self.import.open = false;
    }

    pub(crate) fn cancel_import_bar(&mut self, cx: &mut Context<Self>) {
        self.close_import_bar();
        self.status = "Canceled the import".into();
        cx.notify();
    }

    pub(crate) fn review_import(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.import.input.clone().filter(|_| self.import.open) else {
            return;
        };
        let text = input.read(cx).content().trim().to_string();
        if text.is_empty() {
            self.status = "Type the path of the file to import".into();
            return;
        }
        let Some(live) = self.save_session() else {
            return;
        };
        let labels = ReviewLabels {
            title: format!("Import {{n}} row(s) into {}?", self.catalog.grid_table.name),
            done: "Imported {n} row(s)".into(),
            review: "Review the import, then import or cancel".into(),
            confirm: "Import",
            note: Some(
                "Fields are matched to columns by name; unmatched fields are ignored. At most 1,000 rows are \
                 imported per run, all in one transaction; running it again repeats the first rows.",
            ),
            reconnect: None,
        };
        let choices = self.import_choices(cx);
        self.prepare(live, SaveRequest::Import(PathBuf::from(text), choices), labels, cx);
    }

    pub(crate) fn import_choices(&self, cx: &App) -> bookie_app_services::csv_import_plan::ImportChoices {
        bookie_app_services::csv_import_plan::ImportChoices {
            delimiter: self.import.delimiter,
            null_marker: self
                .import
                .null_input
                .as_ref()
                .map(|input| input.read(cx).content().to_string())
                .unwrap_or_default(),
        }
    }

    pub(crate) fn import_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        use bookie_core::export::CsvDelimiter;
        let input = self.import.input.clone()?;
        if !self.import.open || self.save.pending.is_some() || self.save.busy {
            return None;
        }
        let choices = [
            (None, "Auto-detect"),
            (Some(CsvDelimiter::Comma), "Comma"),
            (Some(CsvDelimiter::Semicolon), "Semicolon"),
            (Some(CsvDelimiter::Tab), "Tab"),
            (Some(CsvDelimiter::Pipe), "Pipe"),
        ];
        let separators: Vec<_> = choices
            .into_iter()
            .enumerate()
            .map(|(index, (delimiter, label))| {
                segment(
                    ("import-separator", index),
                    label,
                    self.import.delimiter == delimiter,
                    theme,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.import.delimiter = delimiter;
                    cx.notify();
                }))
            })
            .collect();
        Some(
            div()
                .debug_selector(|| "import-form".into())
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(format!("Import a file into {}", self.catalog.grid_table.name)),
                        )
                        .child(
                            div()
                                .text_size(px(size::TEXT_SMALL))
                                .text_color(rgb(theme.muted_foreground))
                                .child("CSV or Excel (.xlsx). Fields are matched to columns by name; you review the rows before anything is written."),
                        ),
                )
                .child(field(theme, "File", &input))
                .child(
                    labelled_row(theme, "Separator", segmented(separators, theme).flex_wrap().h_auto())
                        .debug_selector(|| "import-separator".into()),
                )
                .children(
                    self.import
                        .null_input
                        .as_ref()
                        .map(|null| field(theme, "NULL as", null).debug_selector(|| "import-null".into())),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            button(ButtonKind::Primary, "review-import", "Review", theme)
                                .on_click(cx.listener(|this, _, _, cx| this.review_import(cx))),
                        )
                        .child(
                            button(ButtonKind::Ghost, "cancel-import", "Cancel", theme)
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_import_bar(cx))),
                        ),
                ),
        )
    }
}
