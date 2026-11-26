use bookie_gpui::keymap::Command;
use bookie_gpui::palette::{GROUPS, known_group};
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
    let (view, cx) = open_live(cx);
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
    let (view, cx) = open_live(cx);
    let actions = view.read_with(cx, |view, _| view.palette_actions());
    let mismatched: Vec<(String, &str)> = actions
        .iter()
        .filter(|action| action.id != "sidebar")
        .filter_map(|action| Command::for_action(action.id).map(|command| (action.label.clone(), command.title())))
        .filter(|(label, title)| label != title)
        .collect();
    assert!(mismatched.is_empty(), "{mismatched:?}");
}

#[gpui::test]
fn every_toolbar_menu_item_is_a_palette_command_with_the_same_name(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
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
    let (view, cx) = open_live(cx);
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
    let (view, cx) = open_live(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("font", window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.palette.open, "the font list opens");
        assert!(view.palette.fonts);
        assert!(view.palette_actions().iter().all(|action| action.id == "font-choice"));
    });
}

#[gpui::test]
fn the_palette_shows_shortcuts_in_the_same_format_as_the_editor_hints(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.action_shortcut("filter").as_deref(), Some("Ctrl+F"));
        assert_eq!(view.action_shortcut("run").as_deref(), Some("Ctrl+Shift+Enter"));
        assert_eq!(view.action_shortcut("go-to-last-edit").as_deref(), Some("Ctrl+Alt+-"));
        assert_eq!(view.action_shortcut("query").as_deref(), Some("Ctrl+T"));
    });
}
