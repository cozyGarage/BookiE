use bookie_app_services::set_cells::MAX_SET_CELLS;
use bookie_gpui::edit::{is_editable, parse_edit};
use bookie_gpui::model::{Cell, CellText};

use super::*;
use crate::chrome::ghost;

#[derive(Default)]
pub(crate) struct SetCellsState {
    pub open: bool,
    pub input: Option<Entity<TextInput>>,
    pub focus: bool,
}

pub(crate) fn staged_values(cells: &[(u64, Cell)], text: Option<&str>) -> Result<Vec<(u64, Cell)>, String> {
    if cells.is_empty() {
        return Err("No cells are selected.".into());
    }
    if cells.len() > MAX_SET_CELLS {
        return Err("That is too many cells to set at once".into());
    }
    let mut staged = Vec::new();
    for (row, current) in cells {
        let value = match text {
            None => Cell::Null,
            Some(text) => parse_edit(current.kind(), text)?,
        };
        if &value != current {
            staged.push((*row, value));
        }
    }
    Ok(staged)
}

impl GridView {
    pub(crate) fn set_cells_enabled(&self) -> bool {
        !self.cell_edit.set.open
            && self.selection.is_some()
            && self.cell_cursor.is_some()
            && self.source.editable()
            && self.compare.is_none()
    }

    pub(crate) fn focus_set_cells_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.cell_edit.set.focus) {
            return;
        }
        if let Some(input) = &self.cell_edit.set.input {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    pub(crate) fn close_set_cells_bar(&mut self) {
        self.cell_edit.set.open = false;
    }

    pub(crate) fn open_set_cells_bar(&mut self, cx: &mut Context<Self>) {
        if !self.set_cells_enabled() || self.form_open() || self.save.pending.is_some() || self.save.busy {
            return;
        }
        let input = self.cell_edit.set.input.get_or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new("Value for the active column of every selected row", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submit) {
                    this.apply_set_cells(false, cx);
                }
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| input.set_content("", cx));
        self.cell_edit.set.open = true;
        self.cell_edit.set.focus = true;
        self.status = "Type the value to set on the selected rows".into();
        cx.notify();
    }

    fn set_cells_targets(&self) -> Option<Vec<(u64, Cell)>> {
        let selection = self.selection?;
        let column = self.cell_cursor?.column;
        let cells = (selection.first()..=selection.last())
            .map(|index| {
                let source = self.source_row(index);
                (source, self.cell_at(source, column))
            })
            .collect::<Vec<_>>();
        let allowed = |cell: &Cell| {
            if self.live.is_some() {
                !matches!(cell, Cell::Undecodable(_))
            } else {
                is_editable(column, cell)
            }
        };
        cells.iter().all(|(_, cell)| allowed(cell)).then_some(cells)
    }

    pub(crate) fn apply_set_cells(&mut self, null: bool, cx: &mut Context<Self>) {
        let Some(input) = self.cell_edit.set.input.clone().filter(|_| self.cell_edit.set.open) else {
            return;
        };
        let Some(column) = self.cell_cursor.map(|cursor| cursor.column) else {
            return;
        };
        let Some(targets) = self.set_cells_targets() else {
            self.status = "A selected cell cannot be edited, so no cells were changed.".into();
            cx.notify();
            return;
        };
        let typed = input.read(cx).content().to_string();
        match staged_values(&targets, (!null).then_some(typed.as_str())) {
            Ok(staged) if staged.is_empty() => self.status = "Every selected cell already has that value".into(),
            Ok(staged) => {
                let count = staged.len();
                for (row, value) in staged {
                    self.record_edit((row, column), value);
                }
                self.edits_changed();
                self.close_set_cells_bar();
                self.status = format!("{count} cell(s) set; review and save to apply");
            }
            Err(reason) => self.status = format!("No cells were changed: {reason}"),
        }
        cx.notify();
    }

    pub(crate) fn set_cells_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.cell_edit.set.input.clone()?;
        if !self.cell_edit.set.open {
            return None;
        }
        let rows = self.selection.map_or(0, |selection| selection.count());
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
                .child(
                    div()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(format!("Set {rows} selected row(s)")),
                )
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    ghost("apply-set-cells", "Set value", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.apply_set_cells(false, cx))),
                )
                .child(
                    ghost("apply-set-null", "Set NULL", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.apply_set_cells(true, cx))),
                )
                .child(
                    ghost("cancel-set-cells", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.close_set_cells_bar();
                        cx.notify();
                    })),
                ),
        )
    }
}

#[cfg(test)]
#[path = "set_cells_ui_tests.rs"]
mod tests;
