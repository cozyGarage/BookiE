use bookie_app_services::structure_change_plan::StructureChangeKind;

use super::*;
use crate::chrome::ghost;
use crate::save_ui::{ReviewLabels, SaveRequest};

#[derive(Default)]
pub(crate) struct StructureState {
    pub open: bool,
    pub kind: Option<StructureChangeKind>,
    pub input: Option<Entity<TextInput>>,
}

fn title(kind: StructureChangeKind) -> &'static str {
    match kind {
        StructureChangeKind::AddColumn => "Add column",
        StructureChangeKind::RenameColumn => "Rename column",
        StructureChangeKind::DropColumn => "Drop column",
        StructureChangeKind::AddIndex => "Create index",
        StructureChangeKind::DropIndex => "Drop index",
    }
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
        self.structure.open = true;
        self.status = format!("{}: {}", title(kind), kind.hint());
        cx.notify();
    }

    pub(crate) fn close_structure_bar(&mut self) {
        self.structure.open = false;
    }

    pub(crate) fn cancel_structure_bar(&mut self, cx: &mut Context<Self>) {
        self.close_structure_bar();
        self.status = "Cancelled the structure change".into();
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
        let name = match kind {
            StructureChangeKind::AddIndex if first.eq_ignore_ascii_case("unique") => words.next().unwrap_or_default(),
            _ => first,
        }
        .to_string();
        let confirm = title(kind);
        let labels = ReviewLabels {
            title: format!("{confirm} {name} on {}?", self.catalog.grid_table.name),
            done: format!("{confirm} {name}: done"),
            review: "Review the change, then apply or cancel".into(),
            confirm,
            note: Some(
                "This changes the table's structure for everyone using the database, and most databases cannot \
                 undo it. A connection that needs approval will ask before it runs.",
            ),
        };
        self.prepare(live, SaveRequest::Structure(kind, text), labels, cx);
    }

    pub(crate) fn structure_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.structure.input.clone()?;
        let kind = self.structure.kind?;
        if !self.structure.open || self.save.pending.is_some() || self.save.busy {
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
                .child(div().font_weight(gpui::FontWeight::MEDIUM).child(title(kind)))
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    ghost("review-structure", "Review", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.review_structure(cx))),
                )
                .child(
                    ghost("cancel-structure", "Cancel", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_structure_bar(cx))),
                ),
        )
    }
}
