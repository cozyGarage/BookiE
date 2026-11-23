use bookie_core::Value;
use gpui::{Context, Div, Stateful, div, prelude::*, px, rgb};

use crate::GridView;
use bookie_gpui::theme::{Theme, size};

#[derive(Clone, Debug)]
pub(crate) struct FkPeekState {
    pub table: String,
    pub fields: Vec<(String, String)>,
    pub sql: String,
    pub params: Vec<Value>,
}

impl GridView {
    pub(crate) fn fk_peek_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let peek = self.fk_peek.as_ref()?;
        let table_name = peek.table.clone();
        let sql = peek.sql.clone();
        let params = peek.params.clone();
        Some(
            div()
                .id("fk-peek-panel")
                .flex_none()
                .max_h(px(size::FK_PEEK_MAX_HEIGHT))
                .p_2()
                .border_t_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.popover))
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pb_1()
                        .border_b_1()
                        .border_color(rgb(theme.border))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme.muted_foreground))
                                        .child("Foreign Key Peek:"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(rgb(theme.foreground))
                                        .child(table_name),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .id("open-fk-tab")
                                        .px_2()
                                        .py_0p5()
                                        .rounded(px(theme.radius))
                                        .bg(rgb(theme.accent))
                                        .text_xs()
                                        .cursor_pointer()
                                        .child("Open in new tab")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.run_sql_with(sql.clone(), params.clone(), cx);
                                            this.fk_peek = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .id("close-fk-peek")
                                        .px_1()
                                        .rounded(px(theme.radius))
                                        .cursor_pointer()
                                        .child(crate::assets::icon("x", theme.muted_foreground))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.fk_peek = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .pt_1()
                        .children(peek.fields.iter().cloned().map(|(col, val)| {
                            div()
                                .flex()
                                .items_start()
                                .gap_2()
                                .py_0p5()
                                .text_xs()
                                .child(
                                    div()
                                        .w(px(size::ROW_DETAIL_LABEL_WIDTH))
                                        .flex_none()
                                        .text_color(rgb(theme.muted_foreground))
                                        .child(col),
                                )
                                .child(div().flex_1().text_color(rgb(theme.foreground)).child(val))
                        })),
                ),
        )
    }
}
