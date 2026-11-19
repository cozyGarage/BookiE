use std::path::PathBuf;

use super::*;
use crate::chrome::ghost;
use crate::save_ui::{ReviewLabels, SaveRequest};

pub(crate) const IMPORT_ROW_CAP: usize = 1_000;

#[derive(Default)]
pub(crate) struct ImportState {
    pub open: bool,
    pub input: Option<Entity<TextInput>>,
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
            let input = cx.new(|cx| TextInput::new("Path of a .csv file", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submit) {
                    this.review_import(cx);
                }
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| input.set_content("", cx));
        self.import.open = true;
        self.status = "Type the path of the file to import, then review it".into();
        cx.notify();
    }

    pub(crate) fn close_import_bar(&mut self) {
        self.import.open = false;
    }

    pub(crate) fn cancel_import_bar(&mut self, cx: &mut Context<Self>) {
        self.close_import_bar();
        self.status = "Cancelled the import".into();
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
        self.prepare(live, SaveRequest::Import(PathBuf::from(text)), labels, cx);
    }

    pub(crate) fn import_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.import.input.clone()?;
        if !self.import.open || self.save.pending.is_some() || self.save.busy {
            return None;
        }
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .p_2()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Import CSV"))
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    ghost("review-import", "Review", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.review_import(cx))),
                )
                .child(
                    ghost("cancel-import", "Cancel", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_import_bar(cx))),
                ),
        )
    }
}
