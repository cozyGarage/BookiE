use bookie_app_services::structure_change_plan::StructureChangeKind;

use super::*;
use crate::save_ui::{ReviewLabels, SaveRequest};
use crate::surfaces::{ButtonKind, button, task_bar, task_bar_frame};

#[derive(Default)]
pub(crate) struct StructureState {
    pub open: bool,
    pub kind: Option<StructureChangeKind>,
    pub info: Option<String>,
    pub input: Option<Entity<TextInput>>,
}

impl GridView {
    pub(crate) fn structure_change_enabled(&self) -> bool {
        self.can_save_edits() && self.cell_edit.edits.is_empty() && self.compare.is_none() && !self.form_open()
    }

    pub(crate) fn open_structure_bar(&mut self, kind: StructureChangeKind, cx: &mut Context<Self>) {
        if self.compare.is_some() {
            self.status = "Leave compare mode before changing the structure".into();
            return;
        }
        if !self.cell_edit.edits.is_empty() {
            self.status = "Save or discard the pending edits before changing the structure".into();
            return;
        }
        if self.form_open() || self.insert.loading || self.save_session().is_none() {
            return;
        }
        let input = self.structure.input.get_or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new("", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submit) {
                    this.review_structure(cx);
                }
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| {
            input.set_placeholder(kind.hint(), cx);
            input.set_content("", cx);
        });
        self.structure.kind = Some(kind);
        self.structure.info = None;
        self.structure.open = true;
        self.status = format!("{}: {}", kind.title(), kind.hint());
        cx.notify();
    }

    pub(crate) fn close_structure_bar(&mut self) {
        self.structure.open = false;
        self.structure.info = None;
    }

    pub(crate) fn foreign_key_change_enabled(&self) -> bool {
        self.structure_change_enabled()
            && self
                .live
                .as_ref()
                .is_some_and(|live| bookie_core::sql_dialect::alters_foreign_keys(&live.driver_id))
    }

    pub(crate) fn structure_info_enabled(&self) -> bool {
        self.live.is_some() && !self.grid_is_query && self.compare.is_none() && self.structure.info.is_none()
    }

    pub(crate) fn show_structure(&mut self, cx: &mut Context<Self>) {
        self.read_structure_panel(false, cx);
    }

    pub(crate) fn show_create_table(&mut self, cx: &mut Context<Self>) {
        self.read_structure_panel(true, cx);
    }

    fn read_structure_panel(&mut self, as_script: bool, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone().filter(|_| self.structure_info_enabled()) else {
            return;
        };
        let table = self.catalog.grid_table.clone();
        self.status = format!("Reading the structure of {}…", table.name);
        cx.spawn(async move |this, cx| {
            let asked = table.clone();
            let owning_connection = live.clone();
            let read = cx
                .background_executor()
                .spawn(async move {
                    let columns = live.table_columns(&table)?;
                    let structure = live.table_structure(&table)?;
                    if as_script {
                        bookie_app_services::structure_summary::create_table_script(
                            &live.driver_id,
                            table.schema.as_deref(),
                            &table.name,
                            &columns,
                            &structure.indexes,
                            &structure.foreign_keys,
                        )
                    } else {
                        Ok(bookie_app_services::structure_summary::structure_summary(
                            &columns,
                            &structure.indexes,
                            &structure.foreign_keys,
                        ))
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                if !this.is_current_session(&owning_connection) || this.catalog.grid_table != asked {
                    return;
                }
                match read {
                    Ok(text) => {
                        this.structure.info = Some(text);
                        this.status = format!("Structure of {}", asked.name);
                    }
                    Err(message) => this.status = format!("Could not read the structure: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn structure_info_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let text = self.structure.info.clone()?;
        Some(
            task_bar_frame(theme)
                .items_start()
                .child(
                    div()
                        .id("structure-info")
                        .flex_1()
                        .min_w_0()
                        .max_h(px(size::STRUCTURE_PANEL_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .text_size(px(size::TEXT_SMALL))
                        .child(text),
                )
                .child(
                    button(ButtonKind::Ghost, "close-structure-info", "Close", theme).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.structure.info = None;
                            cx.notify();
                        },
                    )),
                ),
        )
    }

    pub(crate) fn cancel_structure_bar(&mut self, cx: &mut Context<Self>) {
        self.close_structure_bar();
        self.status = "Canceled the structure change".into();
        cx.notify();
    }

    pub(crate) fn review_structure(&mut self, cx: &mut Context<Self>) {
        let (Some(input), Some(kind)) = (
            self.structure.input.clone().filter(|_| self.structure.open),
            self.structure.kind,
        ) else {
            return;
        };
        let text = input.read(cx).content().trim().to_string();
        let Some(live) = self.save_session() else {
            return;
        };
        let mut words = text.split_whitespace();
        let first = words.next().unwrap_or_default();
        let unique = kind == StructureChangeKind::AddIndex && first.eq_ignore_ascii_case("unique");
        let name = if unique {
            words.next().unwrap_or_default()
        } else {
            first
        }
        .to_string();
        let confirm = if unique { "Create unique index" } else { kind.title() };
        let labels = ReviewLabels {
            title: format!("{confirm} {name} on {}?", self.catalog.grid_table.name),
            done: format!("{confirm} {name}: done"),
            review: "Review the change, then apply or cancel".into(),
            confirm,
            note: Some(
                if matches!(
                    kind,
                    StructureChangeKind::TruncateTable | StructureChangeKind::DropTable
                ) {
                    "This permanently deletes data and cannot be undone. Rows and tables that depend on it may be \
                 affected, and a connection that needs approval will ask before it runs."
                } else if kind == StructureChangeKind::AddForeignKey {
                    "This checks every existing row and can block writes to both tables while it runs. Deleting or \
                 changing a referenced row will fail afterwards. Most databases cannot undo it, and a connection \
                 that needs approval will ask before it runs."
                } else {
                    "This changes the table's structure for everyone using the database, and most databases cannot \
                 undo it. A connection that needs approval will ask before it runs."
                },
            ),
            reconnect: match kind {
                StructureChangeKind::RenameTable => Some(bookie_core::TableInfo {
                    schema: self.catalog.grid_table.schema.clone(),
                    name: name.clone(),
                }),
                StructureChangeKind::DropTable => Some(self.catalog.grid_table.clone()),
                _ => None,
            },
        };
        self.prepare(live, SaveRequest::Structure(kind, text), labels, cx);
    }

    pub(crate) fn review_diagram_foreign_key(&mut self, key: bookie_core::ForeignKeyInfo, cx: &mut Context<Self>) {
        if self.transaction_open() || self.reject_view_edits() {
            cx.notify();
            return;
        }
        let Some(live) = self.save_session() else {
            cx.notify();
            return;
        };
        let table = table_label(&self.catalog.grid_table);
        let labels = ReviewLabels {
            title: format!("Add foreign key {} on {table}?", key.name),
            done: format!("Added foreign key {} on {table}", key.name),
            review: "Review the relationship change, then apply or cancel".into(),
            confirm: "Add foreign key",
            note: Some("The database checks existing rows and may block writes while validating the new constraint."),
            reconnect: None,
        };
        self.prepare(live, SaveRequest::DiagramForeignKey(key), labels, cx);
    }

    pub(crate) fn structure_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.structure.input.clone()?;
        let kind = self.structure.kind?;
        if !self.structure.open || self.save.pending.is_some() || self.save.busy {
            return None;
        }
        Some(
            task_bar(kind.title(), theme)
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    button(ButtonKind::Primary, "review-structure", "Review", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.review_structure(cx))),
                )
                .child(
                    button(ButtonKind::Ghost, "cancel-structure", "Cancel", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_structure_bar(cx))),
                ),
        )
    }
}
