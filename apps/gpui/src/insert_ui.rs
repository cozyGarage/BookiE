use bookie_core::ColumnInfo;

use super::*;
use crate::chrome::ghost;
use crate::save_ui::{ReviewLabels, SaveRequest};

pub(crate) struct InsertField {
    pub name: String,
    pub input: Entity<TextInput>,
}

#[derive(Default)]
pub(crate) struct InsertState {
    pub open: bool,
    pub loading: bool,
    pub fields: Vec<InsertField>,
}

impl GridView {
    pub(crate) fn open_insert_form(&mut self, cx: &mut Context<Self>) {
        if self.compare.is_some() {
            self.status = "Leave compare mode before adding a row".into();
            return;
        }
        if !self.cell_edit.edits.is_empty() {
            self.status = "Save or discard the pending edits before adding a row".into();
            return;
        }
        if self.form_open() || self.insert.loading {
            return;
        }
        let Some(live) = self.save_session() else {
            return;
        };
        self.insert.loading = true;
        let table = self.catalog.grid_table.clone();
        let asked = (self.save.generation, table.clone(), live.writable);
        self.status = "Reading the table's columns…".into();
        cx.spawn(async move |this, cx| {
            let columns = cx
                .background_executor()
                .spawn(async move { live.table_columns(&table) })
                .await;
            this.update(cx, |this, cx| {
                this.insert.loading = false;
                let now = (
                    this.save.generation,
                    this.catalog.grid_table.clone(),
                    this.live.as_ref().and_then(|live| live.writable),
                );
                if now != asked || this.import.open || this.structure.open {
                    this.status.clear();
                    cx.notify();
                    return;
                }
                match columns {
                    Ok(columns) => this.show_insert_form(&columns, cx),
                    Err(message) => this.status = format!("Cannot add a row: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn show_insert_form(&mut self, columns: &[ColumnInfo], cx: &mut Context<Self>) {
        let fields: Vec<InsertField> = columns
            .iter()
            .filter(|column| !column.is_auto_increment && !column.is_generated)
            .map(|column| {
                let hint = match &column.default_value {
                    Some(_) => format!("{} (default if empty)", column.data_type),
                    None => column.data_type.clone(),
                };
                let input = cx.new(|cx| TextInput::new(&hint, cx));
                cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Submit) {
                        this.review_insert(cx);
                    }
                })
                .detach();
                InsertField {
                    name: column.name.clone(),
                    input,
                }
            })
            .collect();
        if fields.is_empty() {
            self.status = "Every column of this table is filled in by the database".into();
            return;
        }
        self.insert.fields = fields;
        self.insert.open = true;
        self.status = "Fill in the new row, then review it".into();
    }

    pub(crate) fn close_insert_form(&mut self) {
        self.insert.open = false;
        self.insert.fields.clear();
    }

    pub(crate) fn cancel_insert_form(&mut self, cx: &mut Context<Self>) {
        self.close_insert_form();
        self.status = "Cancelled the new row".into();
        cx.notify();
    }

    pub(crate) fn review_insert(&mut self, cx: &mut Context<Self>) {
        if !self.insert.open {
            return;
        }
        let Some(live) = self.save_session() else {
            return;
        };
        let values: Vec<(String, String)> = self
            .insert
            .fields
            .iter()
            .map(|field| (field.name.clone(), field.input.read(cx).content().to_string()))
            .collect();
        let labels = ReviewLabels {
            title: format!("Insert 1 row into {}?", self.catalog.grid_table.name),
            done: "Inserted 1 row".into(),
            review: "Review the new row, then insert or cancel".into(),
            confirm: "Insert",
            note: None,
            reconnect: false,
        };
        self.prepare(live, SaveRequest::Insert(values), labels, cx);
    }

    pub(crate) fn insert_form(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        if !self.insert.open || self.save.pending.is_some() || self.save.busy {
            return None;
        }
        let fields = self.insert.fields.iter().map(|field| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(div().text_color(rgb(theme.muted_foreground)).child(field.name.clone()))
                .child(div().w(px(size::FORM_CONTROL * 4.0)).child(field.input.clone()))
        });
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().font_weight(gpui::FontWeight::MEDIUM).child("New row"))
                        .child(
                            ghost("review-insert", "Review", theme)
                                .on_click(cx.listener(|this, _, _, cx| this.review_insert(cx))),
                        )
                        .child(
                            ghost("cancel-insert", "Cancel", theme)
                                .on_click(cx.listener(|this, _, _, cx| this.cancel_insert_form(cx))),
                        ),
                )
                .child(div().flex().flex_wrap().gap_3().children(fields))
                .child(
                    div()
                        .text_size(px(size::TEXT_SMALL))
                        .text_color(rgb(theme.muted_foreground))
                        .child("Leave a field empty to let the database fill it in. Press Enter in a field to review."),
                ),
        )
    }
}
