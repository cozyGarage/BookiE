use bookie_core::TableInfo;
use gpui::{Context, Div, div, prelude::*, px, rgb};

use crate::GridView;
use crate::assets::icon;
use crate::sidebar_state::{FieldRow, FieldState};
use bookie_gpui::theme::{Theme, size};

impl GridView {
    pub(crate) fn toggle_fields(&mut self, table: &TableInfo, cx: &mut Context<Self>) {
        if let Some(index) = self.sidebar.fields.iter().position(|(open, _)| open == table) {
            self.sidebar.fields.remove(index);
            return;
        }
        let Some(session) = self.live.clone() else {
            return;
        };
        self.sidebar.fields.push((table.clone(), FieldState::Loading));
        let asked = table.clone();
        let started_by = session.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn({
                    let asked = asked.clone();
                    async move { session.table_columns(&asked) }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.is_current_session(&started_by) {
                    return;
                }
                let Some((_, state)) = this.sidebar.fields.iter_mut().find(|(open, _)| *open == asked) else {
                    return;
                };
                *state = match result {
                    Ok(columns) => FieldState::Loaded(
                        columns
                            .into_iter()
                            .map(|column| FieldRow {
                                name: column.name,
                                data_type: column.data_type,
                                primary_key: column.primary_key,
                            })
                            .collect(),
                    ),
                    Err(message) => FieldState::Failed(message),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn field_rows(&self, table: &TableInfo, depth: usize, theme: Theme) -> Vec<Div> {
        let Some((_, state)) = self.sidebar.fields.iter().find(|(open, _)| open == table) else {
            return Vec::new();
        };
        let indent = size::SM + size::LG * (depth + 1) as f32;
        let line = |marker: Option<&'static str>, name: String, detail: String, dim: bool| {
            div()
                .flex()
                .items_center()
                .h(px(size::TREE_ROW))
                .pl(px(indent))
                .pr(px(size::SM))
                .gap_1()
                .text_size(px(size::TEXT_SMALL))
                .text_color(rgb(if dim { theme.muted_foreground } else { theme.foreground }))
                .child(
                    div()
                        .w(px(size::CHEVRON))
                        .flex_none()
                        .children(marker.map(|name| icon(name, theme.muted_foreground))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(name),
                )
                .child(div().text_color(rgb(theme.muted_foreground)).child(detail))
        };
        match state {
            FieldState::Loading => vec![line(None, "Loading fields…".into(), String::new(), true)],
            FieldState::Failed(message) => vec![line(None, message.clone(), String::new(), true)],
            FieldState::Loaded(fields) => fields
                .iter()
                .map(|field| {
                    line(
                        field.primary_key.then_some("key-round"),
                        field.name.clone(),
                        field.data_type.clone(),
                        false,
                    )
                })
                .collect(),
        }
    }
}
