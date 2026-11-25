use bookie_core::Value;
use gpui::{Context, Div, Stateful, div, prelude::*, rgb};

use crate::GridView;
use crate::surfaces::{ButtonKind, compact_button, compact_icon_button};
use bookie_gpui::theme::Theme;

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
                .flex_1()
                .min_h_0()
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
                                    compact_button(ButtonKind::Ghost, "open-fk-tab", "Run in query results", theme)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.clear_fk_peek();
                                            this.run_sql_with(sql.clone(), params.clone(), cx);
                                            cx.notify();
                                        })),
                                )
                                .child(compact_icon_button("close-fk-peek", "x", "Close", theme).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.clear_fk_peek();
                                        cx.notify();
                                    }),
                                )),
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
                                .flex_col()
                                .py_0p5()
                                .text_xs()
                                .child(div().text_color(rgb(theme.muted_foreground)).child(col))
                                .child(div().min_w_0().text_color(rgb(theme.foreground)).child(val))
                        })),
                ),
        )
    }
}
