use bookie_gpui::detail::{Detail, detail_of};
use bookie_gpui::theme::{Theme, size};
use gpui::{Div, Stateful, div, prelude::*, px, relative, rgb};
use gpui_component::checkbox::Checkbox;

use crate::GridView;
use bookie_gpui::model::{Cell, ColumnMeta};

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
        let dataset_index = self.base().checked_add(u64::try_from(cursor.row).ok()?)?;
        if dataset_index >= self.dataset_count() {
            return None;
        }
        let source = self.source_row(dataset_index);
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

    pub(super) fn row_detail_target(&self, filter: &str, hide_null: bool) -> Option<Vec<(usize, String, String)>> {
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
        Some(filtered_fields(self.source.columns(), &row, filter, hide_null))
    }

    pub(super) fn detail_sidebar_width(&self, available: f32) -> f32 {
        if !(self.detail_open || self.row_detail_open || self.fk_peek.is_some()) {
            return 0.0;
        }
        (available * 0.36).clamp(size::DETAIL_SIDEBAR_MIN_WIDTH, size::DETAIL_SIDEBAR_WIDTH)
    }

    pub(super) fn detail_sidebar(&self, theme: Theme, cx: &mut gpui::Context<Self>) -> Option<Stateful<Div>> {
        let panels: Vec<Stateful<Div>> = [
            self.detail_panel(theme),
            self.row_detail_panel(theme, cx),
            self.fk_peek_panel(theme, cx),
        ]
        .into_iter()
        .flatten()
        .collect();
        if panels.is_empty() {
            return None;
        }
        Some(
            div()
                .id("detail-sidebar")
                .debug_selector(|| "detail-sidebar".into())
                .flex_none()
                .w(relative(0.36))
                .min_w(px(size::DETAIL_SIDEBAR_MIN_WIDTH))
                .max_w(px(size::DETAIL_SIDEBAR_WIDTH))
                .flex()
                .flex_col()
                .min_h_0()
                .border_l_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.muted))
                .children(panels),
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
                .debug_selector(|| "cell-detail".into())
                .flex_1()
                .min_h_0()
                .p_2()
                .overflow_y_scroll()
                .child(header)
                .child(body),
        )
    }

    pub(super) fn row_detail_panel(&self, theme: Theme, cx: &mut gpui::Context<Self>) -> Option<Stateful<Div>> {
        if !self.row_detail_open {
            return None;
        }
        let filter = self.row_detail_filter.read(cx).content().to_string();
        let body = self.row_detail_target(&filter, self.row_detail_hide_null).map_or_else(
            || {
                div()
                    .text_color(rgb(theme.muted_foreground))
                    .child("Select a row to view its values.")
            },
            |fields| {
                if fields.is_empty() {
                    return div()
                        .debug_selector(|| "row-detail-empty".into())
                        .text_color(rgb(theme.muted_foreground))
                        .child("No fields to show. Clear the filter or turn off Hide NULL fields.");
                }
                div()
                    .flex()
                    .flex_col()
                    .children(fields.into_iter().map(|(column, name, value)| {
                        div()
                            .id(format!("row-detail-field-{column}"))
                            .debug_selector(move || format!("row-detail-field-{column}"))
                            .flex()
                            .flex_col()
                            .py_1()
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(size::TEXT_SMALL))
                                    .text_color(rgb(theme.muted_foreground))
                                    .child(name),
                            )
                            .child(div().min_w_0().child(value))
                    }))
            },
        );
        let controls = div()
            .tab_group()
            .flex()
            .items_center()
            .gap_2()
            .pb_2()
            .child(
                div()
                    .id("row-detail-filter")
                    .debug_selector(|| "row-detail-filter".into())
                    .flex_1()
                    .min_w_0()
                    .child(self.row_detail_filter.clone()),
            )
            .child(
                div().debug_selector(|| "row-detail-hide-null".into()).child(
                    Checkbox::new("row-detail-hide-null")
                        .label("Hide NULL fields")
                        .checked(self.row_detail_hide_null)
                        .on_change(cx.listener(|this, checked, _, cx| {
                            this.row_detail_hide_null = *checked;
                            cx.notify();
                        })),
                ),
            );
        Some(
            div()
                .id("row-detail")
                .debug_selector(|| "row-detail".into())
                .flex_1()
                .min_h_0()
                .p_2()
                .overflow_y_scroll()
                .child(
                    div()
                        .pb_1()
                        .text_color(rgb(theme.muted_foreground))
                        .child("Row details"),
                )
                .child(controls)
                .child(body),
        )
    }
}

fn filtered_fields(
    columns: &[ColumnMeta],
    row: &[Cell],
    filter: &str,
    hide_null: bool,
) -> Vec<(usize, String, String)> {
    let filter = filter.to_lowercase();
    columns
        .iter()
        .enumerate()
        .filter_map(|(index, column)| {
            let cell = row.get(index);
            if hide_null && matches!(cell, Some(Cell::Null)) {
                return None;
            }
            let value = cell.map_or_else(|| "Missing value".into(), bookie_gpui::preview::cell_preview);
            if !filter.is_empty()
                && !column.name.to_lowercase().contains(&filter)
                && !value.to_lowercase().contains(&filter)
            {
                return None;
            }
            Some((index, column.name.clone(), value))
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use bookie_core::Value;

    use super::filtered_fields;
    use bookie_gpui::model::ColumnMeta;

    fn column(name: &str) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            width: 100.0,
            numeric: false,
            data_type: String::new(),
            primary_key: false,
        }
    }

    #[test]
    fn row_details_filter_names_and_display_values_without_losing_typed_nulls() {
        let columns = [
            column("tag"),
            column("tag"),
            column("empty"),
            column("optional"),
            column("payload"),
            column("unicode"),
        ];
        let row = [
            Value::Text("first".into()),
            Value::Text("second".into()),
            Value::Text(String::new()),
            Value::Null,
            Value::Undecodable("numeric".into()),
            Value::Text("Café 🧪".into()),
        ];

        assert_eq!(
            filtered_fields(&columns, &row, "tag", false)
                .iter()
                .map(|(index, name, _)| (*index, name.as_str()))
                .collect::<Vec<_>>(),
            [(0, "tag"), (1, "tag")]
        );
        assert_eq!(filtered_fields(&columns, &row, "🧪", false)[0].0, 5);
        assert_eq!(filtered_fields(&columns, &row, "CAFÉ", false)[0].0, 5);
        assert_eq!(filtered_fields(&columns, &row, "undecodable", false)[0].0, 4);

        let visible = filtered_fields(&columns, &row, "", true);
        assert_eq!(visible.iter().map(|field| field.0).collect::<Vec<_>>(), [0, 1, 2, 4, 5]);
        assert_eq!(visible[2].2, "");
        assert_eq!(visible[3].2, "<undecodable numeric>");
        assert!(filtered_fields(&columns, &row, "NULL", true).is_empty());
        assert_eq!(filtered_fields(&columns, &row, "NULL", false)[0].0, 3);
    }
}
