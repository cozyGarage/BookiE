use bookie_gpui::detail::{Detail, detail_of};
use bookie_gpui::theme::{Theme, size};
use gpui::{Div, Stateful, div, prelude::*, px, rgb};

use crate::GridView;

const PANEL_HEIGHT: f32 = 200.0;
const MONO: &str = "monospace";

impl GridView {
    pub(super) fn toggle_detail(&mut self) {
        self.detail_open = !self.detail_open;
        self.row_detail_open = false;
    }

    pub(super) fn toggle_row_detail(&mut self) {
        self.row_detail_open = !self.row_detail_open;
        self.detail_open = false;
    }

    pub(super) fn detail_target(&self) -> Option<(String, Detail)> {
        let cursor = self.cell_cursor?;
        let source = self.source_row(self.base() + cursor.row as u64);
        let name = self.source.columns().get(cursor.column)?.name.clone();
        let cell = self.cell_at(source, cursor.column);
        let detail = match (&cell, self.binary_mode_for_column(cursor.column)) {
            (bookie_gpui::model::Cell::Bytes(bytes), mode) if mode != crate::binary_mode::BinaryMode::Hex => {
                Detail::Text(crate::binary_mode::detail_text(bytes, mode))
            }
            _ => detail_of(&cell),
        };
        Some((name, detail))
    }

    pub(super) fn row_detail_target(&self) -> Option<Vec<(String, String)>> {
        if self.compare.is_some() {
            return None;
        }
        let cursor = self.cell_cursor?;
        let dataset_index = self.base().checked_add(u64::try_from(cursor.row).ok()?)?;
        if dataset_index >= self.dataset_count() || cursor.column >= self.source.columns().len() {
            return None;
        }
        let source_row = self.source_row(dataset_index);
        let mut row = self.source.row(source_row);
        for (column, value) in row.iter_mut().enumerate() {
            if let Some(edited_value) = self.cell_edit.edits.get(&(source_row, column)) {
                *value = edited_value.clone();
            }
        }
        Some(
            self.source
                .columns()
                .iter()
                .enumerate()
                .map(|(column, info)| {
                    (
                        info.name.clone(),
                        row.get(column).map_or_else(
                            || "Missing value".into(),
                            bookie_gpui::preview::cell_preview,
                        ),
                    )
                })
                .collect(),
        )
    }

    pub(super) fn detail_panel(&self, theme: Theme) -> Option<Stateful<Div>> {
        if !self.detail_open {
            return None;
        }
        let (header, body) = match self.detail_target() {
            None => (
                div().text_color(rgb(theme.muted_foreground)).child("No cell selected"),
                div()
                    .text_color(rgb(theme.muted_foreground))
                    .child("Select a cell to view its contents."),
            ),
            Some((name, detail)) => {
                let header = div().text_color(rgb(theme.muted_foreground)).child(name);
                (header, detail_body(detail, theme))
            }
        };
        Some(
            div()
                .id("cell-detail")
                .h(px(PANEL_HEIGHT))
                .flex_none()
                .p_2()
                .border_t_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.muted))
                .overflow_y_scroll()
                .child(header)
                .child(body),
        )
    }

    pub(super) fn row_detail_panel(&self, theme: Theme) -> Option<Stateful<Div>> {
        if !self.row_detail_open {
            return None;
        }
        let body = self.row_detail_target().map_or_else(
            || {
                div()
                    .text_color(rgb(theme.muted_foreground))
                    .child("Select a row to view its values.")
            },
            |fields| {
                div()
                    .flex()
                    .flex_col()
                    .children(fields.into_iter().map(|(name, value)| {
                        div()
                            .flex()
                            .items_start()
                            .gap_3()
                            .py_1()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .w(px(size::ROW_DETAIL_LABEL_WIDTH))
                                    .flex_none()
                                    .text_color(rgb(theme.muted_foreground))
                                    .child(name),
                            )
                            .child(div().flex_1().min_w_0().child(value))
                    }))
            },
        );
        Some(
            div()
                .id("row-detail")
                .h(px(size::ROW_DETAIL_HEIGHT))
                .flex_none()
                .p_2()
                .border_t_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.muted))
                .overflow_y_scroll()
                .child(
                    div()
                        .pb_1()
                        .text_color(rgb(theme.muted_foreground))
                        .child("Row details"),
                )
                .child(body),
        )
    }
}

fn detail_body(detail: Detail, theme: Theme) -> Div {
    match detail {
        Detail::Null => div().child("NULL").italic().text_color(rgb(theme.muted_foreground)),
        Detail::Text(text) => div().child(text),
        Detail::Json { pretty } => div().font_family(MONO).child(pretty),
        Detail::Hex {
            lines,
            total,
            shown,
            decoded,
        } => div()
            .font_family(MONO)
            .flex()
            .flex_col()
            .children(
                lines
                    .into_iter()
                    .map(|line| div().child(format!("{:08x}  {:<47}  {}", line.offset, line.hex, line.ascii))),
            )
            .when_some(decoded, |panel, (label, decoded)| {
                panel.child(
                    div()
                        .mt_2()
                        .child(div().text_color(rgb(theme.muted_foreground)).child(label))
                        .child(decoded),
                )
            })
            .when(shown < total, |panel| {
                panel.child(
                    div()
                        .text_color(rgb(theme.muted_foreground))
                        .child(format!("showing {shown} of {total} bytes")),
                )
            }),
    }
}
