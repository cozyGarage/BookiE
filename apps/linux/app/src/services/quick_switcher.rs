pub use bookie_app_services::quick_switcher::{QuickItem, QuickTarget, favorite_items, filter};
use bookie_core::TableInfo;

pub fn relation_items(tables: &[TableInfo], views: &[TableInfo]) -> Vec<QuickItem> {
    bookie_app_services::quick_switcher::relation_items(tables, views, &bookie_app_services::tr!("Table"), &bookie_app_services::tr!("View"))
}

fn commands() -> Vec<(&'static str, String)> {
    vec![
        ("open-editor", bookie_app_services::tr!("New SQL tab")),
        ("new-window", bookie_app_services::tr!("New window")),
        ("show-history", bookie_app_services::tr!("Query history")),
        ("show-catalog", bookie_app_services::tr!("Catalog objects")),
        ("show-activity", bookie_app_services::tr!("Server activity")),
        ("explain-query", bookie_app_services::tr!("Explain query")),
        ("jump-column", bookie_app_services::tr!("Jump to column")),
        ("open-filter", bookie_app_services::tr!("Filter rows")),
        ("refresh-page", bookie_app_services::tr!("Refresh the current page")),
        ("save-changes", bookie_app_services::tr!("Save changes")),
        ("undo-change", bookie_app_services::tr!("Undo change")),
        ("redo-change", bookie_app_services::tr!("Redo change")),
        ("reopen-closed-tab", bookie_app_services::tr!("Reopen closed tab")),
        ("export-csv", bookie_app_services::tr!("Export results as CSV")),
        ("export-json", bookie_app_services::tr!("Export results as JSON")),
        ("export-all-csv", bookie_app_services::tr!("Export all rows as CSV")),
        ("export-all-json", bookie_app_services::tr!("Export all rows as JSON")),
        ("preferences", bookie_app_services::tr!("Preferences")),
        ("shortcuts", bookie_app_services::tr!("Keyboard shortcuts")),
    ]
}

pub fn command_items() -> Vec<QuickItem> {
    commands()
        .into_iter()
        .map(|(action, title)| QuickItem {
            target: QuickTarget::Command(action),
            title,
            subtitle: bookie_app_services::tr!("Command"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_command_names_an_action_the_window_registers() {
        let registered = include_str!("../ui/app/shortcuts.rs");
        for (action, _) in commands() {
            assert!(registered.contains(&format!("\"{action}\"")), "{action}");
        }
    }
}
