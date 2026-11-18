use bookie_core::CatalogObjectKind;
use gpui::{Context, Div, Stateful, div, prelude::*, px, rgb};

use crate::GridView;
use crate::assets::icon;
use crate::sidebar_state::ObjectState;
use bookie_gpui::theme::{Theme, size};

pub(crate) fn kind_label(kind: CatalogObjectKind) -> &'static str {
    match kind {
        CatalogObjectKind::Routine => "Functions and procedures",
        CatalogObjectKind::Trigger => "Triggers",
        CatalogObjectKind::Sequence => "Sequences",
        CatalogObjectKind::Extension => "Extensions",
        CatalogObjectKind::Role => "Roles",
        CatalogObjectKind::Type => "Types",
        CatalogObjectKind::Grant => "Privileges",
    }
}

impl GridView {
    pub(crate) fn toggle_catalog_kind(&mut self, kind: CatalogObjectKind, cx: &mut Context<Self>) {
        if let Some(index) = self.sidebar.catalog.iter().position(|(open, _)| *open == kind) {
            self.sidebar.catalog.remove(index);
            return;
        }
        let Some(session) = self.live.clone() else {
            return;
        };
        self.sidebar.catalog.push((kind, ObjectState::Loading));
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { session.catalog_objects(kind) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some((_, state)) = this.sidebar.catalog.iter_mut().find(|(open, _)| *open == kind) else {
                    return;
                };
                *state = match result {
                    Ok(objects) => ObjectState::Loaded(objects),
                    Err(message) => ObjectState::Failed(message),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn catalog_rows(&self, theme: Theme, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let Some(session) = self.live.as_ref() else {
            return Vec::new();
        };
        let kinds = session.catalog_kinds();
        if kinds.is_empty() {
            return Vec::new();
        }
        let line = |id: gpui::ElementId, depth: f32, label: String, detail: String, chevron: Option<bool>| -> Stateful<Div> {
            div()
                .id(id)
                .flex()
                .items_center()
                .h(px(size::TREE_ROW))
                .pl(px(size::SM + size::LG * depth))
                .pr(px(size::SM))
                .gap_1()
                .rounded(px(theme.radius))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(theme.accent)))
                .child(div().w(px(size::CHEVRON)).flex_none().children(chevron.map(|open| {
                    icon(if open { "chevron-down" } else { "chevron-right" }, theme.muted_foreground)
                })))
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label))
                .child(div().text_size(px(size::TEXT_SMALL)).text_color(rgb(theme.muted_foreground)).child(detail))
        };
        let mut rows = vec![
            line("catalog".into(), 0.0, "Catalog".into(), kinds.len().to_string(), Some(self.sidebar.catalog_open))
                .font_weight(gpui::FontWeight::MEDIUM)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.sidebar.catalog_open = !this.sidebar.catalog_open;
                    cx.notify();
                }))
                .into_any_element(),
        ];
        if !self.sidebar.catalog_open {
            return rows;
        }
        for (index, kind) in kinds.iter().copied().enumerate() {
            let state = self.sidebar.catalog.iter().find(|(open, _)| *open == kind).map(|(_, state)| state);
            let count = match state {
                Some(ObjectState::Loaded(objects)) => objects.len().to_string(),
                _ => String::new(),
            };
            rows.push(
                line(("catalog-kind", index).into(), 1.0, kind_label(kind).into(), count, Some(state.is_some()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_catalog_kind(kind, cx);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            match state {
                Some(ObjectState::Loading) => rows.push(note("Loading…", theme)),
                Some(ObjectState::Failed(message)) => rows.push(note(message, theme)),
                Some(ObjectState::Loaded(objects)) if objects.is_empty() => rows.push(note("Nothing of this kind was found.", theme)),
                Some(ObjectState::Loaded(objects)) => rows.extend(objects.iter().map(|object| object_row(object, theme))),
                None => {}
            }
        }
        rows
    }
}

fn object_row(object: &bookie_core::CatalogObject, theme: Theme) -> gpui::AnyElement {
    let name = match &object.schema {
        Some(schema) => format!("{schema}.{}", object.name),
        None => object.name.clone(),
    };
    div()
        .flex()
        .items_center()
        .h(px(size::TREE_ROW))
        .pl(px(size::SM + size::LG * 2.0 + size::CHEVRON))
        .pr(px(size::SM))
        .text_size(px(size::TEXT_SMALL))
        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(name))
        .child(div().text_color(rgb(theme.muted_foreground)).child(object.detail.clone().unwrap_or_default()))
        .into_any_element()
}

fn note(text: &str, theme: Theme) -> gpui::AnyElement {
    div()
        .h(px(size::TREE_ROW))
        .pl(px(size::SM + size::LG * 2.0 + size::CHEVRON))
        .text_size(px(size::TEXT_SMALL))
        .text_color(rgb(theme.muted_foreground))
        .child(text.to_string())
        .into_any_element()
}
