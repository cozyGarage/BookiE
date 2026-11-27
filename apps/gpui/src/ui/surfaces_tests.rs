use super::*;
use crate::ui_theme::ThemeExt;
use gpui::{Context, TestAppContext, Window};
use gpui_kit::test::TestQueryExt;

struct Probe;

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::light();
        div()
            .flex()
            .child(icon_button("probe", "refresh-cw", "Refresh", theme))
            .child(compact_icon_button("probe-compact", "x", "Close tab", theme))
    }
}

struct MenuProbe;

impl Render for MenuProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};

        menu_button(
            "probe-menu",
            Some(div().child("Open").into_any_element()),
            None,
            "Open menu",
            "Choose an option",
            true,
        )
        .debug_selector(|| "probe-menu".into())
        .dropdown_menu(|menu, _, _| menu.item(PopupMenuItem::new("Choice")))
    }
}

#[gpui::test]
fn menu_button_exposes_its_name_tooltip_and_dropdown(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::light().install(cx);
    });
    let (_view, cx) = cx.add_window_view(|_, _| MenuProbe);
    cx.run_until_parked();

    let control = cx.debug_bounds("probe-menu").expect("menu button is drawn");
    assert_eq!(control.size.height, px(size::COMPACT_CONTROL));
    let semantics = cx.update(|window, _| window.find_by_label("Open menu"));
    assert_eq!(semantics.description(), Some("Choose an option"));

    cx.simulate_click(control.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.try_find_by_label("Choice").is_some()));
}

#[gpui::test]
fn an_icon_button_carries_its_tooltip_on_the_control(cx: &mut TestAppContext) {
    let (_view, cx) = cx.add_window_view(|_, _| Probe);
    cx.run_until_parked();
    let control = cx.debug_bounds("probe").expect("the icon button is drawn");
    let marker = cx
        .debug_bounds("probe tooltip: Refresh")
        .expect("the tooltip is attached to the icon button");
    assert!(
        control.contains(&marker.center()),
        "the tooltip marker sits on its control"
    );
    assert_eq!(control.size.height, px(size::SMALL_CONTROL));
    let compact = cx
        .debug_bounds("probe-compact")
        .expect("the compact icon button is drawn");
    assert_eq!(compact.size.height, px(size::COMPACT_CONTROL));
    assert!(cx.debug_bounds("probe-compact tooltip: Close tab").is_some());
}

#[test]
fn the_tooltip_marker_names_the_control_and_its_tooltip() {
    assert_eq!(
        icon_selector("header-refresh", "Refresh"),
        "header-refresh tooltip: Refresh"
    );
}

#[test]
fn button_kinds_look_different() {
    for theme in [Theme::light(), Theme::dark()] {
        let primary = ButtonKind::Primary.colors(theme);
        let ghost = ButtonKind::Ghost.colors(theme);
        let destructive = ButtonKind::Destructive.colors(theme);
        assert_eq!(primary.fill, Some(theme.foreground));
        assert_eq!(primary.text, theme.background);
        assert_eq!(ghost.fill, None);
        assert_eq!(ghost.text, theme.foreground);
        assert_eq!(destructive.fill, None);
        assert_eq!(destructive.text, theme.destructive);
        assert_ne!(primary, ghost);
        assert_ne!(ghost, destructive);
        let mut filled = button(ButtonKind::Primary, "primary", "Save", theme);
        let mut flat = button(ButtonKind::Ghost, "ghost", "Cancel", theme);
        assert_eq!(
            filled.style().background,
            Some(gpui::Fill::from(gpui::Hsla::from(rgb(theme.foreground))))
        );
        assert!(flat.style().background.is_none());
    }
}

#[test]
fn an_unavailable_primary_is_not_filled_like_an_available_one() {
    let theme = Theme::light();
    let mut busy = unavailable_button(ButtonKind::Primary, "busy", "Saving…", "Saving the connection", theme);
    assert_eq!(
        busy.style().background,
        Some(gpui::Fill::from(gpui::Hsla::from(rgb(theme.muted))))
    );
}

#[test]
fn paired_buttons_share_a_minimum_width() {
    let theme = Theme::light();
    let mut short = button(ButtonKind::Ghost, "short", "OK", theme);
    let mut long = button(ButtonKind::Primary, "long", "Apply", theme);
    assert_eq!(short.style().min_size.width, long.style().min_size.width);
    assert!(short.style().min_size.width.is_some());
}

#[test]
fn a_selected_segment_is_filled_and_an_unselected_one_is_not() {
    for theme in [Theme::light(), Theme::dark()] {
        let mut selected = segment("on", "Table", true, theme);
        let mut unselected = segment("off", "Chart", false, theme);
        assert_eq!(
            selected.style().background,
            Some(gpui::Fill::from(gpui::Hsla::from(rgb(theme.background))))
        );
        assert!(unselected.style().background.is_none());
        assert!(selected.style().border_widths.left.is_none());
    }
}

#[test]
fn a_failed_list_state_uses_the_destructive_colour() {
    let theme = Theme::light();
    assert_eq!(ListState::Failed("x").colour(theme), theme.destructive);
    assert_eq!(ListState::Empty("x").colour(theme), theme.muted_foreground);
}

struct ConfirmProbe;

impl Render for ConfirmProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::light();
        let dialog = ConfirmDialog {
            id: "probe",
            title: "Drop table orders?".into(),
            body: "This cannot be undone.".into(),
            sql: Some("DROP TABLE orders".into()),
        };
        div().size_full().child(confirm_dialog(
            dialog,
            [
                button(ButtonKind::Destructive, "probe-drop", "Drop", theme),
                button(ButtonKind::Ghost, "probe-cancel", "Cancel", theme),
            ],
            theme,
        ))
    }
}

#[gpui::test]
fn a_confirm_dialog_shows_its_sql_above_actions_in_order(cx: &mut TestAppContext) {
    let (_view, cx) = cx.add_window_view(|_, _| ConfirmProbe);
    cx.run_until_parked();
    let dialog = cx.debug_bounds("probe-dialog").expect("the dialog is drawn");
    let actions = cx.debug_bounds("probe-actions").expect("the actions are drawn");
    let drop = cx.debug_bounds("probe-drop").expect("the destructive action is drawn");
    let cancel = cx.debug_bounds("probe-cancel").expect("cancel is drawn");
    assert!(dialog.contains(&actions.center()));
    assert!(drop.origin.x < cancel.origin.x, "the action comes before Cancel");
}

#[test]
fn a_sql_preview_scrolls_within_its_height() {
    let mut preview = sql_preview("probe-sql", "SELECT 1", size::SAVE_PREVIEW_MAX_HEIGHT, Theme::light());
    assert_eq!(
        preview.style().max_size.height,
        Some(gpui::Length::Definite(px(size::SAVE_PREVIEW_MAX_HEIGHT).into()))
    );
}

struct BarMenuProbe;

impl Render for BarMenuProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
        let theme = Theme::light();
        div()
            .flex()
            .child(
                icon_menu_button("probe-icon-menu", "ellipsis", None, "More", Bar::Small, theme)
                    .dropdown_menu(|menu, _, _| menu.item(PopupMenuItem::new("Choice"))),
            )
            .child(
                select_button("probe-select", Some("Contains"), "Operator", Bar::Compact, theme)
                    .dropdown_menu(|menu, _, _| menu.item(PopupMenuItem::new("Choice"))),
            )
    }
}

#[gpui::test]
fn bar_menu_buttons_take_the_bar_height_and_name_themselves_by_their_tooltip(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::light().install(cx);
    });
    let (_view, cx) = cx.add_window_view(|_, _| BarMenuProbe);
    cx.run_until_parked();
    let icon_menu = cx.debug_bounds("probe-icon-menu").expect("the icon menu is drawn");
    assert_eq!(icon_menu.size.height, px(size::SMALL_CONTROL));
    let marker = cx
        .debug_bounds("probe-icon-menu tooltip: More")
        .expect("the icon carries the tooltip marker");
    assert!(icon_menu.contains(&marker.center()));
    let select = cx.debug_bounds("probe-select").expect("the select button is drawn");
    assert_eq!(select.size.height, px(size::COMPACT_CONTROL));
    assert!(cx.update(|window, _| window.try_find_by_label("Operator").is_some()));
    assert!(cx.update(|window, _| window.try_find_by_label("More").is_some()));
}
