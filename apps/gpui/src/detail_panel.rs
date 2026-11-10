use bookie_gpui::detail::{Detail, detail_of};
use bookie_gpui::theme::Theme;
use gpui::{Div, Stateful, div, prelude::*, px, rgb};

use crate::GridView;

const PANEL_HEIGHT: f32 = 200.0;
const MONO: &str = "monospace";

impl GridView {
    pub(super) fn toggle_detail(&mut self) {
        self.detail_open = !self.detail_open;
    }

    pub(super) fn detail_target(&self) -> Option<(String, Detail)> {
        let cursor = self.cell_cursor?;
        let source = self.source_row(self.base() + cursor.row as u64);
        let name = self.source.columns().get(cursor.column)?.name.clone();
        Some((name, detail_of(&self.cell_at(source, cursor.column))))
    }

    pub(super) fn detail_panel(&self, theme: Theme) -> Option<Stateful<Div>> {
        if !self.detail_open {
            return None;
        }
        let (name, detail) = self.detail_target().unwrap_or_else(|| (String::new(), Detail::Null));
        let body = match detail {
            Detail::Null => div().child("NULL").italic().text_color(rgb(theme.muted_foreground)),
            Detail::Text(text) => div().child(text),
            Detail::Json { pretty } => div().font_family(MONO).child(pretty),
            Detail::Hex { lines, total, shown } => div()
                .font_family(MONO)
                .flex()
                .flex_col()
                .children(
                    lines
                        .into_iter()
                        .map(|line| div().child(format!("{:08x}  {:<47}  {}", line.offset, line.hex, line.ascii))),
                )
                .when(shown < total, |panel| {
                    panel.child(
                        div()
                            .text_color(rgb(theme.muted_foreground))
                            .child(format!("showing {shown} of {total} bytes")),
                    )
                }),
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
                .child(div().text_color(rgb(theme.muted_foreground)).child(name))
                .child(body),
        )
    }
}
