pub struct Action {
    pub id: &'static str,
    pub label: String,
    pub enabled: bool,
}

impl Action {
    pub fn menu_label(&self) -> &str {
        menu_label(self.id).unwrap_or(&self.label)
    }
}

pub fn menu_label(id: &str) -> Option<&'static str> {
    let label = match id {
        "query" => "New query",
        "run" => "Run all",
        "run-at-cursor" => "Run statement",
        "explain" => "Explain",
        "save-favorite" => "Save as favorite",
        "data-dictionary" => "Data dictionary",
        "sidebar" => "Sidebar",
        "results" => "Results pane",
        "detail" => "Detail panel",
        "theme" => "Dark theme",
        "font" => "Font…",
        "new-connection" => "New connection…",
        "home" => "Home",
        "history" => "History",
        "audit" => "Audit log",
        "settings" => "Settings",
        "status" => "Connection status",
        "server-status" => "Server status",
        "import-connections" => "Import…",
        "export-connections" => "Export…",
        "open-sql-file" => "Open SQL file…",
        "save-sql-file" => "Save SQL file…",
        "export-file" => "Export to file…",
        "export-csv" => "Copy page",
        "import-csv" => "Import file…",
        _ => return grid_menu_label(id),
    };
    Some(label)
}

fn grid_menu_label(id: &str) -> Option<&'static str> {
    let label = match id {
        "copy" => "Copy as TSV",
        "copy-csv" => "Copy as CSV",
        "copy-json" => "Copy as JSON",
        "copy-markdown" => "Copy as Markdown",
        "copy-insert" => "Copy as INSERT",
        "copy-in-list" => "Copy IN list",
        "show-row-vertically" => "Row details",
        "selection-stats" => "Statistics",
        "count-table-rows" => "Count rows",
        "edit" => "Edit cell",
        "set-cells" => "Set cells…",
        "clear-selection" => "Clear selection",
        "delete-rows" => "Delete rows…",
        "insert-row" => "Add row…",
        "filter" => "Filter rows",
        "first" => "First page",
        "last" => "Last page",
        "jump-column" => "Jump to column…",
        "fit-visible-columns" => "Fit columns",
        "reset-grid-widths" => "Reset widths",
        "show-hidden-columns" => "Show hidden columns",
        "refresh-table" => "Refresh",
        "compare" => "Compare",
        "continuous" => "Scroll continuously",
        "append-pages" => "Append pages",
        _ => return None,
    };
    Some(label)
}

pub fn opens_dialog(id: &str) -> bool {
    matches!(
        id,
        "new-connection"
            | "import-connections"
            | "export-connections"
            | "quick-switcher"
            | "jump-column"
            | "shortcuts"
            | "favorites"
            | "delete-favorite"
            | "saved-filters"
            | "delete-saved-filter"
            | "save-filter"
            | "schema-context"
            | "font"
            | "select-theme"
            | "open-sql-file"
            | "save-sql-file"
            | "export-file"
            | "import-csv"
            | "set-cells"
            | "insert-row"
            | "delete-rows"
            | "save-changes"
            | "resolve-outcome"
            | "filter-rule"
            | "search-values"
            | "schema-compare"
            | "export-schema-diagram"
            | "backup-database"
            | "restore-database"
            | "compare-table-data"
            | "export-all-tables"
    ) || structure_dialog(id)
}

fn structure_dialog(id: &str) -> bool {
    matches!(
        id,
        "empty-table"
            | "drop-table"
            | "add-column"
            | "rename-column"
            | "drop-column"
            | "add-index"
            | "drop-index"
            | "rename-table"
            | "add-foreign-key"
            | "drop-foreign-key"
    )
}

pub const GROUPS: [&str; 5] = ["Table", "Query", "Connection", "View", "App"];

pub fn group(id: &str) -> &'static str {
    known_group(id).unwrap_or("App")
}

pub fn known_group(id: &str) -> Option<&'static str> {
    if id.starts_with("health-") {
        return Some("Connection");
    }
    let group = match id {
        "query" | "run" | "stop-query" | "run-at-cursor" | "explain" | "favorites" | "save-favorite"
        | "delete-favorite" | "previous-run" | "next-run" | "close-run" | "compare" | "open-sql-file"
        | "save-sql-file" => "Query",
        "new-connection"
        | "import-connections"
        | "export-connections"
        | "reconnect"
        | "disconnect"
        | "status"
        | "sessions"
        | "blocking-locks"
        | "long-running"
        | "replication-lag"
        | "server-variables"
        | "server-status"
        | "cancel-process"
        | "check-tcp"
        | "kafka-topics"
        | "sample-pubsub"
        | "follow-pubsub"
        | "stop-follow"
        | "backup-database"
        | "restore-database"
        | "schema-context"
        | "data-dictionary"
        | "search-values"
        | "show-schema-diagram"
        | "export-schema-diagram"
        | "schema-compare"
        | "export-all-tables" => "Connection",
        "sidebar"
        | "collapse-sidebar-sections"
        | "sidebar-active-only"
        | "notifications"
        | "sidebar-regex"
        | "results"
        | "detail"
        | "show-row-vertically"
        | "wrap-text"
        | "continuous"
        | "font"
        | "font-choice"
        | "select-theme"
        | "theme"
        | "home"
        | "history"
        | "audit"
        | "recent-tab"
        | "reopen-tab" => "View",
        "settings" | "shortcuts" | "keyboard-shortcuts" | "about" | "check-for-updates" | "new-window" => "App",
        _ => return table_group(id),
    };
    Some(group)
}

fn table_group(id: &str) -> Option<&'static str> {
    matches!(
        id,
        "table"
            | "quick-switcher"
            | "jump-column"
            | "fit-visible-columns"
            | "reset-grid-widths"
            | "show-hidden-columns"
            | "reveal-sidebar"
            | "refresh-table"
            | "first"
            | "append-pages"
            | "previous"
            | "next"
            | "last"
            | "find"
            | "filter"
            | "clear-filter"
            | "filter-rule"
            | "where"
            | "clear-where"
            | "save-filter"
            | "saved-filters"
            | "delete-saved-filter"
            | "count-table-rows"
            | "compare-table-data"
            | "go-to-last-edit"
            | "edit"
            | "set-cells"
            | "paste-cells"
            | "paste-literal-cell"
            | "save-changes"
            | "undo-edit"
            | "redo-edit"
            | "delete-rows"
            | "insert-row"
            | "import-csv"
            | "resolve-outcome"
            | "discard-view-edits"
            | "copy"
            | "copy-csv"
            | "copy-json"
            | "copy-markdown"
            | "copy-in-list"
            | "copy-insert"
            | "clear-selection"
            | "selection-stats"
            | "export-csv"
            | "export-file"
            | "set-csv-delimiter"
            | "reset-csv-delimiter"
            | "set-export-limit"
            | "reset-export-limit"
            | "show-structure"
            | "show-create-table"
            | "go-to-reference"
            | "peek-foreign-key"
            | "show-referencing"
            | "add-column"
            | "rename-column"
            | "drop-column"
            | "add-index"
            | "drop-index"
            | "rename-table"
            | "add-foreign-key"
            | "drop-foreign-key"
            | "empty-table"
            | "drop-table"
    )
    .then_some("Table")
}

pub fn fuzzy_score(query: &str, text: &str) -> u32 {
    let needle = query.trim().to_lowercase();
    let hay: Vec<char> = text.to_lowercase().chars().collect();
    if needle.is_empty() {
        return 1;
    }
    let (mut at, mut score, mut previous) = (0usize, 0u32, None::<usize>);
    for letter in needle.chars() {
        let Some(found) = hay[at..].iter().position(|c| *c == letter).map(|offset| offset + at) else {
            return 0;
        };
        score += if previous.is_some_and(|p| found == p + 1) { 3 } else { 1 };
        if found == 0 || hay[found - 1] == ' ' {
            score += 2;
        }
        previous = Some(found);
        at = found + 1;
    }
    if text.to_lowercase().starts_with(&needle) {
        score += 10;
    }
    score
}

pub fn matches(query: &str, actions: &[Action]) -> Vec<usize> {
    let mut scored: Vec<(usize, u32)> = actions
        .iter()
        .enumerate()
        .filter(|(_, action)| action.enabled)
        .map(|(order, action)| (order, fuzzy_score(query, &action.label)))
        .filter(|(_, score)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(order, _)| order).collect()
}

pub fn step(index: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        0
    } else if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions() -> Vec<Action> {
        [
            ("first", "Go to first page", true),
            ("last", "Go to last page", true),
            ("filter", "Filter rows", true),
            ("clear", "Clear filter", false),
            ("copy", "Copy selected rows", true),
        ]
        .into_iter()
        .map(|(id, label, enabled)| Action {
            id,
            label: label.into(),
            enabled,
        })
        .collect()
    }

    fn ids(found: Vec<usize>, list: &[Action]) -> Vec<&'static str> {
        found.into_iter().map(|i| list[i].id).collect()
    }

    #[test]
    fn an_empty_query_lists_every_enabled_action_in_order() {
        let list = actions();
        assert_eq!(ids(matches("", &list), &list), ["first", "last", "filter", "copy"]);
    }

    #[test]
    fn letters_must_appear_in_order_and_disabled_actions_never_show() {
        let list = actions();
        assert_eq!(ids(matches("lpg", &list), &list), ["last"]);
        assert_eq!(ids(matches("gtp", &list), &list), ["first", "last"]);
        assert!(matches("zzz", &list).is_empty());
        assert!(matches("clear", &list).is_empty(), "a disabled action is hidden");
    }

    #[test]
    fn a_prefix_and_consecutive_letters_rank_higher() {
        assert!(fuzzy_score("filter", "Filter rows") > fuzzy_score("filter", "Clear filter now"));
        assert_eq!(fuzzy_score("abc", "xyz"), 0);
    }

    #[test]
    fn commands_group_by_the_object_they_act_on() {
        assert_eq!(group("add-column"), "Table");
        assert_eq!(group("save-changes"), "Table");
        assert_eq!(group("run"), "Query");
        assert_eq!(group("reconnect"), "Connection");
        assert_eq!(group("health-bloat"), "Connection");
        assert_eq!(group("sidebar"), "View");
        assert_eq!(group("settings"), "App");
        assert_eq!(known_group("not-a-command"), None);
        assert!(GROUPS.contains(&group("not-a-command")));
    }

    #[test]
    fn the_selection_wraps_both_ways_and_survives_an_empty_list() {
        assert_eq!(step(0, 3, false), 2);
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(5, 0, true), 0);
    }
}
