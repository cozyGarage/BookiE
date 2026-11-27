use bookie_gpui::theme::{Theme, size};
use gpui::{AnyElement, Div, ElementId, Role, SharedString, Stateful, div, prelude::*, px, rgb};
pub(crate) use gpui_component::button::Button;
use gpui_component::button::ButtonVariants as _;
use gpui_component::{Disableable as _, Selectable as _};
use gpui_kit::component::Sizable as _;

use crate::assets::icon;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ButtonKind {
    Primary,
    Ghost,
    Destructive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ButtonColors {
    pub fill: Option<u32>,
    pub text: u32,
}

impl ButtonKind {
    pub(crate) fn colors(self, theme: Theme) -> ButtonColors {
        match self {
            ButtonKind::Primary => ButtonColors {
                fill: Some(theme.foreground),
                text: theme.background,
            },
            ButtonKind::Ghost => ButtonColors {
                fill: None,
                text: theme.foreground,
            },
            ButtonKind::Destructive => ButtonColors {
                fill: None,
                text: theme.destructive,
            },
        }
    }

    fn unavailable_colors(self, theme: Theme) -> ButtonColors {
        ButtonColors {
            fill: (self == ButtonKind::Primary).then_some(theme.muted),
            text: theme.muted_foreground,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Bar {
    Compact,
    Small,
}

impl Bar {
    pub(crate) fn height(self) -> f32 {
        match self {
            Bar::Compact => size::COMPACT_CONTROL,
            Bar::Small => size::SMALL_CONTROL,
        }
    }

    fn min_button_width(self) -> f32 {
        match self {
            Bar::Compact => size::COMPACT_CONTROL,
            Bar::Small => size::BUTTON_MIN_WIDTH,
        }
    }

    fn button_padding(self) -> f32 {
        match self {
            Bar::Compact => size::SM,
            Bar::Small => size::MD,
        }
    }
}

pub(crate) fn tip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |window, cx| gpui_component::tooltip::Tooltip::new(text.clone()).build(window, cx)
}

fn truncation_tooltip(text: &SharedString) -> Option<SharedString> {
    (!text.trim().is_empty()).then(|| text.clone())
}

pub(crate) fn truncated(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    let id = id.into();
    let selector = id.to_string();
    let text = text.into();
    div()
        .id(id)
        .debug_selector(move || selector)
        .min_w_0()
        .truncate()
        .when_some(truncation_tooltip(&text), |label, full| label.tooltip(tip(full)))
        .child(text)
}

pub(crate) fn icon_selector(id: &str, tooltip: &str) -> String {
    format!("{id} tooltip: {tooltip}")
}

pub(crate) fn icon_button(
    id: impl Into<ElementId>,
    name: &'static str,
    tooltip: impl Into<SharedString>,
    theme: Theme,
) -> Button {
    toggle_icon_button(id, name, tooltip, false, Bar::Small, theme)
}

pub(crate) fn compact_icon_button(
    id: impl Into<ElementId>,
    name: &'static str,
    tooltip: impl Into<SharedString>,
    theme: Theme,
) -> Button {
    toggle_icon_button(id, name, tooltip, false, Bar::Compact, theme)
}

pub(crate) fn toggle_icon_button(
    id: impl Into<ElementId>,
    name: &'static str,
    tooltip: impl Into<SharedString>,
    on: bool,
    bar: Bar,
    theme: Theme,
) -> Button {
    let id = id.into();
    let tooltip = tooltip.into();
    let selector = id.to_string();
    let marker = icon_selector(&selector, &tooltip);
    Button::new(id)
        .debug_selector(move || selector)
        .ghost()
        .accessibility_label(tooltip.clone())
        .tooltip(tooltip)
        .size(px(bar.height()))
        .w(px(bar.height()))
        .h(px(bar.height()))
        .p_0()
        .rounded(px(theme.radius))
        .cursor_pointer()
        .selected(on)
        .toggled(on)
        .text_color(rgb(if on { theme.foreground } else { theme.muted_foreground }))
        .when(on, |button| button.bg(rgb(theme.accent)))
        .child(
            div()
                .debug_selector(move || marker)
                .child(icon(name, if on { theme.foreground } else { theme.muted_foreground })),
        )
}

pub(crate) fn link_colour(theme: Theme) -> u32 {
    theme.info
}

pub(crate) fn environment_color(environment: bookie_core::Environment, theme: Theme) -> Option<u32> {
    match environment {
        bookie_core::Environment::Local => None,
        bookie_core::Environment::Dev => Some(theme.success),
        bookie_core::Environment::Staging => Some(theme.warning),
        bookie_core::Environment::Prod => Some(theme.destructive),
    }
}

pub(crate) enum Leading {
    Icon(&'static str),
    Element(AnyElement),
}

pub(crate) struct MenuTrigger<'a> {
    tooltip: SharedString,
    bar: Bar,
    leading: Option<Leading>,
    label: Option<&'a str>,
    accessible_name: Option<SharedString>,
}

impl<'a> MenuTrigger<'a> {
    pub(crate) fn new(tooltip: impl Into<SharedString>, bar: Bar) -> Self {
        Self {
            tooltip: tooltip.into(),
            bar,
            leading: None,
            label: None,
            accessible_name: None,
        }
    }

    pub(crate) fn icon(mut self, name: &'static str) -> Self {
        self.leading = Some(Leading::Icon(name));
        self
    }

    pub(crate) fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading = Some(Leading::Element(element.into_any_element()));
        self
    }

    pub(crate) fn label(mut self, text: impl Into<Option<&'a str>>) -> Self {
        self.label = text.into();
        self
    }

    pub(crate) fn accessible_name(mut self, name: impl Into<SharedString>) -> Self {
        self.accessible_name = Some(name.into());
        self
    }
}

pub(crate) fn menu_button(id: impl Into<ElementId>, trigger: MenuTrigger<'_>, theme: Theme) -> Button {
    let id = id.into();
    let selector = id.to_string();
    let marker = icon_selector(&selector, &trigger.tooltip);
    let name = trigger.accessible_name.unwrap_or_else(|| trigger.tooltip.clone());
    let leading = trigger.leading.map(|leading| match leading {
        Leading::Icon(name) => icon(name, theme.muted_foreground).into_any_element(),
        Leading::Element(element) => element,
    });
    let height = px(trigger.bar.height());
    let content = div()
        .flex()
        .flex_auto()
        .items_center()
        .min_w_0()
        .gap_1()
        .children(leading.map(|leading| div().flex_none().debug_selector(move || marker).child(leading)))
        .children(trigger.label.map(|text| {
            div()
                .flex_auto()
                .min_w_0()
                .truncate()
                .text_color(rgb(theme.foreground))
                .child(text.to_string())
        }));
    let button = Button::new(id)
        .debug_selector(move || selector)
        .ghost()
        .small()
        .accessibility_label(name)
        .tooltip(trigger.tooltip)
        .dropdown_caret(true);
    let button = if trigger.bar == Bar::Compact {
        button.compact()
    } else {
        button
    };
    let button = match trigger.label {
        None => button.p_0().size(height),
        Some(_) => button.h(height).min_w(height),
    };
    button.child(content)
}

pub(crate) fn select_button(
    id: impl Into<ElementId>,
    label: Option<&str>,
    tooltip: impl Into<SharedString>,
    bar: Bar,
    theme: Theme,
) -> Button {
    menu_button(id, MenuTrigger::new(tooltip, bar).label(label), theme)
}

pub(crate) fn button(
    kind: ButtonKind,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: Theme,
) -> Button {
    sized_button(kind, id, label, Bar::Small, theme)
}

pub(crate) fn compact_button(
    kind: ButtonKind,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: Theme,
) -> Button {
    sized_button(kind, id, label, Bar::Compact, theme)
}

fn sized_button(
    kind: ButtonKind,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    bar: Bar,
    theme: Theme,
) -> Button {
    let colors = kind.colors(theme);
    button_frame(id, label.into(), kind, colors, bar, theme).cursor_pointer()
}

pub(crate) fn unavailable_button(
    kind: ButtonKind,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    reason: impl Into<SharedString>,
    theme: Theme,
) -> Button {
    button_frame(
        id,
        label.into(),
        kind,
        kind.unavailable_colors(theme),
        Bar::Small,
        theme,
    )
    .cursor_default()
    .disabled(true)
    .tooltip(reason)
}

fn button_frame(
    id: impl Into<ElementId>,
    label: SharedString,
    kind: ButtonKind,
    colors: ButtonColors,
    bar: Bar,
    theme: Theme,
) -> Button {
    let id = id.into();
    let selector = id.to_string();
    let button = match kind {
        ButtonKind::Primary => Button::new(id).primary(),
        ButtonKind::Ghost | ButtonKind::Destructive => Button::new(id).ghost(),
    }
    .debug_selector(move || selector)
    .label(label)
    .h(px(bar.height()))
    .min_w(px(bar.min_button_width()))
    .max_w(px(size::BUTTON_MAX_WIDTH))
    .px(px(bar.button_padding()))
    .overflow_hidden()
    .rounded(px(theme.radius))
    .text_color(rgb(colors.text));
    button.when_some(colors.fill, |button, fill| button.bg(rgb(fill)))
}

pub(crate) fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    theme: Theme,
) -> Button {
    let label = label.into();
    let selector = label.to_string();
    Button::new(id)
        .debug_selector(move || selector)
        .ghost()
        .accessibility_label(label.clone())
        .selected(selected)
        .toggled(selected)
        .gap_1()
        .h(px(size::SEGMENT))
        .px_2()
        .rounded(px(theme.radius))
        .whitespace_nowrap()
        .when(selected, |segment| {
            segment.bg(rgb(theme.background)).text_color(rgb(theme.foreground))
        })
        .when(!selected, |segment| segment.text_color(rgb(theme.muted_foreground)))
        .child(div().min_w_0().overflow_hidden().text_ellipsis().child(label))
}

pub(crate) fn segmented<I, E>(segments: I, theme: Theme) -> Div
where
    I: IntoIterator<Item = E>,
    E: IntoElement,
{
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(size::OPTICAL))
        .h(px(size::COMPACT_CONTROL))
        .p(px(size::OPTICAL))
        .rounded(px(theme.radius))
        .bg(rgb(theme.muted))
        .children(segments)
}

pub(crate) fn scrim(theme: Theme) -> Div {
    let base = if theme.dark { theme.background } else { theme.foreground };
    let mut colour = rgb(base);
    colour.a = 0.55;
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(colour)
}

pub(crate) fn dialog_surface(width: f32, theme: Theme) -> Div {
    div()
        .w(px(width))
        .max_w_full()
        .flex()
        .flex_col()
        .rounded(px(size::SURFACE_RADIUS))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.popover))
        .text_color(rgb(theme.foreground))
        .shadow_lg()
}

pub(crate) fn sql_preview(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    max_h: f32,
    theme: Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .max_h(px(max_h))
        .overflow_y_scroll()
        .p_2()
        .bg(rgb(theme.chrome))
        .rounded(px(theme.radius))
        .font_family("monospace")
        .text_size(px(size::TEXT_SMALL))
        .child(text.into())
}

pub(crate) struct ConfirmDialog {
    pub id: &'static str,
    pub title: SharedString,
    pub body: SharedString,
    pub sql: Option<SharedString>,
}

pub(crate) fn confirm_dialog(
    dialog: ConfirmDialog,
    actions: impl IntoIterator<Item = Button>,
    theme: Theme,
) -> Stateful<Div> {
    let id = dialog.id;
    let surface = format!("{id}-dialog");
    let selector = surface.clone();
    scrim(theme).id(format!("{id}-overlay")).occlude().child(
        dialog_surface(size::APPROVAL_DIALOG_WIDTH, theme)
            .id(surface)
            .debug_selector(move || selector)
            .role(Role::Dialog)
            .aria_label(dialog.title.clone())
            .max_w(gpui::relative(0.92))
            .max_h(gpui::relative(0.9))
            .gap_2()
            .p_3()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(dialog.title))
            .child(
                div()
                    .id(format!("{id}-detail"))
                    .max_h(px(size::SAVE_PREVIEW_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .text_size(px(size::TEXT_SMALL))
                    .text_color(rgb(theme.muted_foreground))
                    .child(dialog.body),
            )
            .children(
                dialog
                    .sql
                    .map(|sql| sql_preview(format!("{id}-sql"), sql, size::SAVE_PREVIEW_MAX_HEIGHT, theme)),
            )
            .child(
                div()
                    .debug_selector(move || format!("{id}-actions"))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .children(actions),
            ),
    )
}

pub(crate) fn task_bar_frame(theme: Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .p_2()
        .flex_none()
        .bg(rgb(theme.chrome))
        .border_b_1()
        .border_color(rgb(theme.border))
}

pub(crate) fn task_title(title: impl Into<SharedString>) -> Div {
    div()
        .flex_none()
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(title.into())
}

pub(crate) fn task_bar(title: impl Into<SharedString>, theme: Theme) -> Div {
    task_bar_frame(theme).child(task_title(title))
}

pub(crate) fn section_header(caption: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .text_size(px(size::TEXT_SMALL))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(theme.muted_foreground))
        .child(caption.into())
}

pub(crate) fn note(text: impl Into<SharedString>, theme: Theme) -> Div {
    div().text_color(rgb(theme.muted_foreground)).child(text.into())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ListState<'a> {
    Loading(&'a str),
    Empty(&'a str),
    Failed(&'a str),
}

impl ListState<'_> {
    fn icon(self) -> &'static str {
        match self {
            ListState::Loading(_) => "refresh-cw",
            ListState::Empty(_) => "search",
            ListState::Failed(_) => "info",
        }
    }

    fn colour(self, theme: Theme) -> u32 {
        match self {
            ListState::Failed(_) => theme.destructive,
            ListState::Loading(_) | ListState::Empty(_) => theme.muted_foreground,
        }
    }

    fn text(self) -> String {
        match self {
            ListState::Loading(text) | ListState::Empty(text) | ListState::Failed(text) => text.to_string(),
        }
    }
}

pub(crate) fn list_state(state: ListState<'_>, theme: Theme) -> Div {
    let colour = state.colour(theme);
    div()
        .flex()
        .items_center()
        .gap_2()
        .p_4()
        .text_color(rgb(colour))
        .child(icon(state.icon(), colour))
        .child(state.text())
}

pub(crate) fn badge(text: impl Into<SharedString>, colour: u32) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .min_w_0()
        .h(px(size::BADGE))
        .px_2()
        .rounded_full()
        .border_1()
        .border_color(rgb(colour))
        .text_size(px(size::TEXT_SMALL))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(colour))
        .child(div().min_w_0().truncate().child(text.into()))
}

pub(crate) fn picker_chip(
    id: impl Into<ElementId>,
    leading: Option<&'static str>,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    theme: Theme,
) -> Stateful<Div> {
    let id = id.into();
    let selector = id.to_string();
    let tooltip = tooltip.into();
    div()
        .id(id)
        .debug_selector(move || selector)
        .role(Role::Button)
        .aria_label(tooltip.clone())
        .tooltip(tip(tooltip))
        .flex()
        .items_center()
        .gap_1()
        .min_w_0()
        .overflow_hidden()
        .h(px(size::SMALL_CONTROL))
        .px_2()
        .rounded(px(theme.radius))
        .border_1()
        .border_color(rgb(theme.border))
        .cursor_pointer()
        .text_color(rgb(theme.muted_foreground))
        .hover(move |style| style.bg(rgb(theme.accent)))
        .children(leading.map(|name| div().flex_none().child(icon(name, theme.muted_foreground))))
        .child(div().min_w_0().truncate().child(label.into()))
}

pub(crate) fn list_row(id: impl Into<ElementId>, selected: bool, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(px(theme.radius))
        .when(selected, |row| {
            row.bg(rgb(theme.accent)).text_color(rgb(theme.foreground))
        })
        .when(!selected, |row| row.hover(move |style| style.bg(rgb(theme.accent))))
}

pub(crate) fn shortcut(keys: Option<String>, theme: Theme) -> Option<Div> {
    let keys = keys.filter(|keys| !keys.is_empty())?;
    let selector = format!("shortcut-keycap-{keys}");
    Some(
        div()
            .debug_selector(move || selector)
            .flex()
            .flex_none()
            .justify_center()
            .px_1()
            .py(px(size::OPTICAL))
            .rounded(px(size::CONTROL_RADIUS))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.chrome))
            .whitespace_nowrap()
            .text_size(px(size::TEXT_SMALL))
            .text_color(rgb(theme.muted_foreground))
            .child(keys),
    )
}

pub(crate) fn card(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(size::SURFACE_RADIUS))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.background))
}

pub(crate) fn group_rows(rows: Vec<Div>, theme: Theme) -> Div {
    let last = rows.len().saturating_sub(1);
    card(theme).children(rows.into_iter().enumerate().map(|(index, row)| {
        let row = row.px_3().py_2();
        if index < last {
            row.border_b_1().border_color(rgb(theme.border))
        } else {
            row
        }
    }))
}

pub(crate) fn group(caption: impl Into<SharedString>, rows: Vec<Div>, theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(section_header(caption, theme))
        .child(group_rows(rows, theme))
}

#[cfg(test)]
#[path = "surfaces_tests.rs"]
mod tests;
