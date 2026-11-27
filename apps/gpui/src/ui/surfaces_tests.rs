use super::*;
use crate::ui_theme::ThemeExt;
use gpui::{Context, TestAppContext, Window};
use gpui_kit::test::TestQueryExt;
use std::{cell::Cell, rc::Rc};

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
            MenuTrigger::new("Choose an option", Bar::Compact)
                .label("Open")
                .accessible_name("Open menu"),
            Theme::light(),
        )
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
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::light().install(cx);
    });
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

struct KeyboardButtonProbe(Rc<Cell<usize>>, gpui::FocusHandle, gpui::FocusHandle);

impl Render for KeyboardButtonProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let first_clicks = self.0.clone();
        let second_clicks = self.0.clone();
        div()
            .tab_group()
            .child(
                button(ButtonKind::Primary, "keyboard-save", "Save", Theme::light())
                    .track_focus(&self.1)
                    .on_click(move |_, _, _| first_clicks.set(first_clicks.get() + 1)),
            )
            .child(
                icon_button("keyboard-close", "x", "Close", Theme::light())
                    .track_focus(&self.2)
                    .on_click(move |_, _, _| second_clicks.set(second_clicks.get() + 1)),
            )
    }
}

#[gpui::test]
fn shared_buttons_are_reachable_and_activate_with_enter_and_space(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::light().install(cx);
    });
    let clicks = Rc::new(Cell::new(0));
    let (view, cx) = cx.add_window_view({
        let clicks = clicks.clone();
        move |_, cx| KeyboardButtonProbe(clicks, cx.focus_handle(), cx.focus_handle())
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    cx.update(|window, cx| window.focus_next(cx));
    assert!(cx.update(|window, cx| { view.read(cx).1.is_focused(window) }));
    let enter = gpui::Keystroke::parse("enter").expect("known button key");
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke: enter });
    assert_eq!(clicks.get(), 1, "Tab reaches the primary button and Enter activates it");

    cx.update(|window, cx| window.focus_next(cx));
    assert!(cx.update(|window, cx| view.read(cx).2.is_focused(window)));
    let space = gpui::Keystroke::parse("space").expect("known button key");
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke: space });
    assert_eq!(clicks.get(), 2, "Tab reaches the icon button and Space activates it");
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
            vec![
                button(ButtonKind::Destructive, "probe-drop", "Drop", theme),
                button(ButtonKind::Ghost, "probe-cancel", "Cancel", theme),
            ],
            theme,
        ))
    }
}

#[gpui::test]
fn a_confirm_dialog_shows_its_sql_above_actions_in_order(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::light().install(cx);
    });
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
        let long = "A very long schema name that cannot fit in the trigger";
        div()
            .flex()
            .child(
                menu_button(
                    "probe-icon-menu",
                    MenuTrigger::new("More", Bar::Small).icon("ellipsis"),
                    theme,
                )
                .dropdown_menu(|menu, _, _| menu.item(PopupMenuItem::new("Choice"))),
            )
            .child(
                select_button("probe-select", Some("Contains"), "Operator", Bar::Compact, theme)
                    .dropdown_menu(|menu, _, _| menu.item(PopupMenuItem::new("Choice"))),
            )
            .child(
                div().w(px(size::BUTTON_MIN_WIDTH)).child(
                    menu_button(
                        "probe-long",
                        MenuTrigger::new("Schema", Bar::Small)
                            .leading(div().size(px(size::SM)))
                            .label(long),
                        theme,
                    )
                    .w_full()
                    .dropdown_menu(|menu, _, _| menu.item(PopupMenuItem::new("Choice"))),
                ),
            )
    }
}

#[gpui::test]
fn menu_buttons_take_the_bar_height_and_name_themselves_by_their_tooltip(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::light().install(cx);
    });
    let (_view, cx) = cx.add_window_view(|_, _| BarMenuProbe);
    cx.run_until_parked();
    let icon_menu = cx.debug_bounds("probe-icon-menu").expect("the icon menu is drawn");
    assert_eq!(icon_menu.size.height, px(size::SMALL_CONTROL));
    assert_eq!(
        icon_menu.size.width,
        px(size::SMALL_CONTROL),
        "an icon-only trigger is square"
    );
    let marker = cx
        .debug_bounds("probe-icon-menu tooltip: More")
        .expect("the icon carries the tooltip marker");
    assert!(icon_menu.contains(&marker.center()));
    let select = cx.debug_bounds("probe-select").expect("the select button is drawn");
    assert_eq!(select.size.height, px(size::COMPACT_CONTROL));
    assert!(cx.update(|window, _| window.try_find_by_label("Operator").is_some()));
    assert!(cx.update(|window, _| window.try_find_by_label("More").is_some()));
    let long = cx.debug_bounds("probe-long").expect("the long trigger is drawn");
    assert_eq!(long.size.height, px(size::SMALL_CONTROL));
    assert!(
        long.size.width <= px(size::BUTTON_MIN_WIDTH),
        "a long label truncates inside the part"
    );
    let marker = cx
        .debug_bounds("probe-long tooltip: Schema")
        .expect("a leading element carries the tooltip marker");
    assert!(long.contains(&marker.center()));
}

const LONG_NAME: &str = "A very long connection name that cannot fit in a narrow column of the window";

struct TruncatedProbe;

impl Render for TruncatedProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::light();
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "probe-column".into())
                    .flex()
                    .w(px(size::BUTTON_MIN_WIDTH))
                    .child(truncated("probe-long-text", LONG_NAME).flex_1()),
            )
            .child(div().flex().child(truncated("probe-short-text", "id")))
            .child(div().w(px(size::BUTTON_MIN_WIDTH)).child(picker_chip(
                "probe-chip",
                Some("database"),
                LONG_NAME,
                "Show the status",
                theme,
            )))
            .child(div().flex().children(shortcut(Some("Ctrl+K".into()), theme)))
            .child(div().debug_selector(|| "probe-group".into()).child(group(
                "Connection",
                vec![
                    div().debug_selector(|| "probe-group-first".into()).child("Host"),
                    div().debug_selector(|| "probe-group-second".into()).child("Port"),
                ],
                theme,
            )))
    }
}

#[gpui::test]
fn truncated_text_stays_on_one_line_inside_its_column(cx: &mut TestAppContext) {
    let (_view, cx) = cx.add_window_view(|_, _| TruncatedProbe);
    cx.run_until_parked();
    let column = cx.debug_bounds("probe-column").expect("the column is drawn");
    let long = cx.debug_bounds("probe-long-text").expect("the long text is drawn");
    let short = cx.debug_bounds("probe-short-text").expect("the short text is drawn");
    assert!(
        long.size.width <= column.size.width,
        "the long text is cut at the column edge"
    );
    assert_eq!(long.size.height, short.size.height, "the long text does not wrap");
}

#[test]
fn truncated_text_carries_its_full_text_as_a_tooltip_unless_it_is_blank() {
    assert_eq!(truncation_tooltip(&LONG_NAME.into()), Some(LONG_NAME.into()));
    assert_eq!(truncation_tooltip(&"".into()), None);
    assert_eq!(truncation_tooltip(&"  ".into()), None);
    let mut text = truncated("probe", LONG_NAME);
    assert_eq!(text.style().overflow.x, Some(gpui::Overflow::Hidden));
    assert_eq!(
        text.style().min_size.width,
        Some(gpui::Length::Definite(px(0.0).into()))
    );
}

#[gpui::test]
fn a_picker_chip_has_the_small_control_height_and_truncates_its_label(cx: &mut TestAppContext) {
    let (_view, cx) = cx.add_window_view(|_, _| TruncatedProbe);
    cx.run_until_parked();
    let chip = cx.debug_bounds("probe-chip").expect("the chip is drawn");
    assert_eq!(chip.size.height, px(size::SMALL_CONTROL));
    assert!(
        chip.size.width <= px(size::BUTTON_MIN_WIDTH),
        "a long label truncates inside the chip"
    );
}

#[test]
fn a_selected_list_row_uses_the_selection_fill_and_others_stay_flat() {
    for theme in [Theme::light(), Theme::dark()] {
        let mut selected = list_row("on", true, theme);
        let mut other = list_row("off", false, theme);
        assert_eq!(
            selected.style().background,
            Some(gpui::Fill::from(gpui::Hsla::from(rgb(theme.accent))))
        );
        assert!(other.style().background.is_none());
        assert!(
            selected.style().border_widths.left.is_none(),
            "selection has no side bar"
        );
    }
}

#[gpui::test]
fn a_shortcut_is_a_keycap_and_a_missing_one_draws_nothing(cx: &mut TestAppContext) {
    let theme = Theme::light();
    assert!(shortcut(None, theme).is_none());
    assert!(shortcut(Some(String::new()), theme).is_none());
    let (_view, cx) = cx.add_window_view(|_, _| TruncatedProbe);
    cx.run_until_parked();
    let keycap = cx.debug_bounds("shortcut-keycap-Ctrl+K").expect("the keycap is drawn");
    assert!(keycap.size.width > px(0.0));
}

#[gpui::test]
fn a_group_draws_its_rows_in_order_inside_one_frame(cx: &mut TestAppContext) {
    let (_view, cx) = cx.add_window_view(|_, _| TruncatedProbe);
    cx.run_until_parked();
    let group = cx.debug_bounds("probe-group").expect("the group is drawn");
    let first = cx.debug_bounds("probe-group-first").expect("the first row is drawn");
    let second = cx.debug_bounds("probe-group-second").expect("the second row is drawn");
    assert!(group.contains(&first.center()) && group.contains(&second.center()));
    assert!(first.origin.y > group.origin.y, "the caption sits above the rows");
    assert!(first.bottom() <= second.origin.y, "rows keep their order");
    let mut frame = card(Theme::light());
    assert_eq!(
        frame.style().corner_radii.top_left,
        Some(px(size::SURFACE_RADIUS).into())
    );
}
