use bookie_gpui::keymap::Command;
use bookie_gpui::palette::{GROUPS, known_group, menu_label, opens_dialog};
use gpui::TestAppContext;

use crate::live_ui_tests::open_live;
use crate::palette_state::PaletteMode;

const PROPER_NAMES: &[&str] = &[
    "BookiE",
    "SQL",
    "CSV",
    "TSV",
    "JSON",
    "PostgreSQL",
    "MySQL",
    "SQLite",
    "DuckDB",
    "Redis",
    "Kafka",
    "MCP",
    "SSH",
    "TCP",
    "ID",
    "ER",
    "URL",
    "IN",
    "INSERT",
    "CREATE",
    "TABLE",
    "WHERE",
    "NULL",
    "Markdown",
    "Mermaid",
    "DBML",
    "Enter",
];

const BRITISH: &[&str] = &["colour", "behaviour", "organisation", "favourite", "cancelled"];

const PALETTE_MODES: [PaletteMode; 13] = [
    PaletteMode::Commands,
    PaletteMode::Tables,
    PaletteMode::Columns,
    PaletteMode::ChartValues,
    PaletteMode::Shortcuts,
    PaletteMode::Favorites,
    PaletteMode::DeleteFavorite,
    PaletteMode::SavedFilters,
    PaletteMode::DeleteSavedFilter,
    PaletteMode::NameFilter,
    PaletteMode::SchemaContext,
    PaletteMode::Themes,
    PaletteMode::CompareTarget,
];

fn copy_problems(text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut sentence_start = true;
    for word in text.split_whitespace() {
        let bare = word.trim_matches(|c: char| !c.is_alphanumeric());
        let capitalised = bare.chars().any(char::is_uppercase);
        if sentence_start && bare.chars().next().is_some_and(char::is_lowercase) && !PROPER_NAMES.contains(&bare) {
            problems.push(format!("{text:?} starts with a lowercase word"));
        }
        if !sentence_start && capitalised && !PROPER_NAMES.contains(&bare) {
            problems.push(format!("{text:?} capitalises {bare:?}"));
        }
        sentence_start = word.ends_with(':') || (sentence_start && bare.is_empty());
    }
    if text.ends_with('.') && !text.ends_with("...") {
        problems.push(format!("{text:?} ends with a period"));
    }
    let lower = text.to_lowercase();
    if ["(ctrl", "(alt", "(shift", "(f1", "(f2", "(f3"]
        .iter()
        .any(|key| lower.contains(key))
    {
        problems.push(format!("{text:?} puts a shortcut in the label"));
    }
    problems.extend(
        BRITISH
            .iter()
            .filter(|spelling| lower.contains(*spelling))
            .map(|spelling| format!("{text:?} uses the British spelling {spelling:?}")),
    );
    problems
}

#[test]
fn the_copy_check_catches_each_rule() {
    assert!(copy_problems("New query").is_empty());
    assert!(copy_problems("Export page or selection as CSV").is_empty());
    assert!(copy_problems("Health: Dead rows and vacuum").is_empty());
    assert!(!copy_problems("New Query").is_empty());
    assert!(!copy_problems("new query").is_empty());
    assert!(!copy_problems("Deleted.").is_empty());
    assert!(!copy_problems("Go to last edit (Ctrl+Alt+-)").is_empty());
    assert!(!copy_problems("Choose colour theme…").is_empty());
}

#[gpui::test]
fn palette_labels_groups_and_shortcut_titles_follow_the_copy_rules(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    let actions = view.read_with(cx, |view, _| view.palette_actions());
    assert!(!actions.is_empty());
    let mut problems: Vec<String> = actions.iter().flat_map(|action| copy_problems(&action.label)).collect();
    problems.extend(GROUPS.iter().flat_map(|group| copy_problems(group)));
    problems.extend(
        Command::ALL
            .into_iter()
            .flat_map(|command| copy_problems(command.title())),
    );
    problems.extend(
        PALETTE_MODES
            .into_iter()
            .flat_map(|mode| copy_problems(mode.placeholder())),
    );
    problems.extend(
        actions
            .iter()
            .filter(|action| known_group(action.id).is_none())
            .map(|action| format!("{} has no palette group", action.id)),
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[gpui::test]
fn a_command_has_one_name_in_the_palette_and_the_shortcut_list(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    let actions = view.read_with(cx, |view, _| view.palette_actions());
    let mismatched: Vec<(String, &str)> = actions
        .iter()
        .filter(|action| !matches!(action.id, "sidebar" | "detail"))
        .filter_map(|action| Command::for_action(action.id).map(|command| (action.label.clone(), command.title())))
        .filter(|(label, title)| label != title)
        .collect();
    assert!(mismatched.is_empty(), "{mismatched:?}");
}

#[gpui::test]
fn every_toolbar_menu_item_is_a_palette_command_with_the_same_name(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    let menus = crate::menus::MENUS;
    view.read_with(cx, |view, _| {
        let actions = view.palette_actions();
        let missing: Vec<&str> = menus
            .iter()
            .flat_map(|groups| groups.iter().flat_map(|group| group.iter().copied()))
            .filter(|id| !actions.iter().any(|action| action.id == *id))
            .collect();
        assert!(missing.is_empty(), "menu items without a palette command: {missing:?}");
    });
}

#[gpui::test]
fn every_menu_item_has_a_handler(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    cx.update(|_, cx| view.update(cx, |view, _| view.connection_bundle.busy = true));
    let ids: Vec<&'static str> = crate::menus::MENUS
        .iter()
        .flat_map(|groups| groups.iter().flat_map(|group| group.iter().copied()))
        .collect();
    let unhandled: Vec<&str> = ids
        .into_iter()
        .filter(|id| !cx.update(|window, cx| view.update(cx, |view, cx| view.execute_palette_action(id, window, cx))))
        .collect();
    assert!(unhandled.is_empty(), "menu items that do nothing: {unhandled:?}");
}

#[gpui::test]
fn choose_font_from_the_menu_opens_the_font_list(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("font", window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.palette.open, "the font list opens");
        assert!(view.palette.fonts);
        assert!(view.palette_actions().iter().all(|action| action.id == "font-choice"));
    });
}

#[gpui::test]
fn the_palette_shows_shortcuts_in_the_same_format_as_the_editor_hints(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.action_shortcut("filter").as_deref(), Some("Ctrl+F"));
        assert_eq!(view.action_shortcut("run").as_deref(), Some("Ctrl+Shift+Enter"));
        assert_eq!(view.action_shortcut("go-to-last-edit").as_deref(), Some("Ctrl+Alt+-"));
        assert_eq!(view.action_shortcut("query").as_deref(), Some("Ctrl+T"));
    });
}

#[gpui::test]
fn a_label_ends_with_an_ellipsis_exactly_when_the_command_asks_for_more(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    let actions = view.read_with(cx, |view, _| view.palette_actions());
    let mut wrong: Vec<String> = actions
        .iter()
        .filter(|action| action.id != "font-choice")
        .flat_map(|action| {
            [
                (action.id, action.label.clone()),
                (action.id, action.menu_label().to_string()),
            ]
        })
        .filter(|(id, label)| label.ends_with('…') != opens_dialog(id))
        .map(|(id, label)| format!("{id}: {label}"))
        .collect();
    wrong.extend(
        Command::ALL
            .into_iter()
            .filter(|command| command.title().ends_with('…') != opens_dialog(command.name()))
            .map(|command| format!("{}: {}", command.name(), command.title())),
    );
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn every_menu_item_has_a_short_menu_label_that_follows_the_copy_rules() {
    let ids = crate::menus::MENUS
        .iter()
        .flat_map(|groups| groups.iter().flat_map(|group| group.iter().copied()));
    let mut problems = Vec::new();
    for id in ids {
        let Some(label) = menu_label(id) else {
            problems.push(format!("{id} has no menu label"));
            continue;
        };
        if label.split_whitespace().count() > 3 {
            problems.push(format!("{id}: {label:?} is longer than three words"));
        }
        problems.extend(copy_problems(label));
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn menu_groups_hold_at_most_seven_items() {
    let long: Vec<&[&str]> = crate::menus::MENUS
        .iter()
        .flat_map(|groups| groups.iter().copied())
        .filter(|group| group.len() > crate::menus::MAX_GROUP_ITEMS)
        .collect();
    assert!(long.is_empty(), "{long:?}");
}

#[gpui::test]
fn menu_items_show_the_keymap_shortcut_and_a_check_for_the_current_state(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    view.read_with(cx, |view, _| {
        let groups = crate::menus::command_groups(view, crate::menus::OVERFLOW_GROUPS);
        let entries: Vec<_> = groups.iter().flatten().collect();
        let entry = |id: &str| entries.iter().find(|entry| entry.id == id).copied().expect("listed");
        assert_eq!(entry("run").shortcut.as_deref(), Some("Ctrl+Shift+Enter"));
        assert_eq!(entry("detail").shortcut.as_deref(), Some("F3"));
        assert_eq!(entry("sidebar").checked, view.sidebar.open);
        assert_eq!(entry("theme").checked, view.dark_mode);
        assert_eq!(entry("home").shortcut, None);
    });
}

#[gpui::test]
fn the_palette_has_one_command_for_the_session_list(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(
                !view
                    .palette_actions()
                    .iter()
                    .any(|action| action.id == "activity-console")
            );
            assert!(!view.execute_palette_action("activity-console", window, cx));
            assert!(!view.execute_palette_action("not-a-command", window, cx));
        })
    });
}
