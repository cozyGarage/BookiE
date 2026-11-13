use bookie_core::QueryResult;
use relm4::adw::prelude::*;
use relm4::{adw, gtk};

use super::PagedExport;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportScope {
    TablePage { filtered: bool },
    TableSnapshot { filtered: bool },
    QueryPage { filtered: bool },
    QueryResult,
    QueryStored,
}

pub(super) fn build(
    scope: ExportScope,
    result: &QueryResult,
    paged: Option<&PagedExport>,
    full_export: bool,
) -> (adw::PreferencesGroup, Option<adw::ComboRow>) {
    let group = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("Scope"))
        .build();
    let choice = match scope {
        ExportScope::TablePage { filtered } => Some(table_choice(filtered, result, full_export)),
        ExportScope::QueryPage { filtered } => Some(query_choice(filtered, result, full_export)),
        _ => None,
    };
    if let Some(choice) = choice {
        group.add(&choice);
        return (group, Some(choice));
    }
    let title = match scope {
        ExportScope::QueryStored => bookie_app_services::tr!("All rows from this query result"),
        ExportScope::QueryPage { .. } => bookie_app_services::tr!("Current query page"),
        ExportScope::TableSnapshot { .. } => bookie_app_services::tr!("All matching table rows"),
        _ => bookie_app_services::tr!("Loaded query result"),
    };
    let subtitle = super::format_subtitle(result, paged);
    group.add(
        &adw::ActionRow::builder()
            .use_markup(false)
            .title(title)
            .subtitle(subtitle)
            .build(),
    );
    (group, None)
}

fn table_choice(filtered: bool, result: &QueryResult, full_export: bool) -> adw::ComboRow {
    let full_label = if full_export {
        bookie_app_services::tr!("All matching table rows")
    } else {
        bookie_app_services::tr!("All matching table rows (unavailable)")
    };
    let choice = choices(bookie_app_services::tr!("Current table page"), full_label);
    let page_description = if filtered {
        bookie_app_services::tr!("{n} loaded rows from the active table filter")
            .replace("{n}", &result.rows.len().to_string())
    } else {
        bookie_app_services::tr!("{n} loaded rows from the current page").replace("{n}", &result.rows.len().to_string())
    };
    choice.set_subtitle(&page_description);
    choice.connect_selected_notify(move |choice| {
        choice.set_subtitle(&if choice.selected() == 0 {
            page_description.clone()
        } else if full_export {
            snapshot_description()
        } else {
            super::file::snapshot_refusal_message().to_string()
        });
    });
    choice
}

fn query_choice(filtered: bool, result: &QueryResult, full_export: bool) -> adw::ComboRow {
    let full_label = query_full_label(filtered, full_export);
    let choice = choices(bookie_app_services::tr!("Current query page"), full_label);
    let page_description = bookie_app_services::tr!("{n} loaded rows from the current query page")
        .replace("{n}", &result.rows.len().to_string());
    choice.set_subtitle(&page_description);
    choice.connect_selected_notify(move |choice| {
        choice.set_subtitle(&query_description(
            choice.selected(),
            filtered,
            full_export,
            &page_description,
        ));
    });
    choice
}

fn query_full_label(filtered: bool, full_export: bool) -> String {
    match (filtered, full_export) {
        (true, true) => bookie_app_services::tr!("All rows in this filtered result"),
        (false, true) => bookie_app_services::tr!("All rows from this query result"),
        (_, false) => bookie_app_services::tr!("All rows from this query result (unavailable)"),
    }
}

fn query_description(selected: u32, filtered: bool, full_export: bool, page: &str) -> String {
    if selected == 0 {
        return page.to_string();
    }
    if !full_export {
        return bookie_app_services::tr!("The complete query result is no longer available");
    }
    if filtered {
        return bookie_app_services::tr!("Every row in the filtered result, read from saved query data");
    }
    bookie_app_services::tr!("Every row in the completed query result, read from saved query data")
}

fn choices(first: String, second: String) -> adw::ComboRow {
    let choices = gtk::StringList::new(&[first.as_str(), second.as_str()]);
    adw::ComboRow::builder()
        .title(bookie_app_services::tr!("Rows to export"))
        .model(&choices)
        .build()
}

pub(super) fn snapshot_description() -> String {
    bookie_app_services::tr!("Every matching row, read in one consistent snapshot taken when the export starts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn table_scope_cannot_enable_unqualified_snapshot_export() {
        adw::init().unwrap();
        let result = QueryResult {
            columns: Vec::new(),
            rows: vec![Vec::new()].into(),
            truncated: true,
        };
        let (group, choice) = build(ExportScope::TablePage { filtered: true }, &result, None, false);
        let choice = choice.unwrap();
        assert_eq!(group.title(), "Scope");
        assert_eq!(choice.selected(), 0);
        let page_description = choice.subtitle();
        choice.set_selected(1);
        assert_ne!(choice.subtitle(), page_description);
        assert_eq!(
            choice.subtitle().unwrap().to_string(),
            super::super::file::snapshot_refusal_message()
        );
        choice.set_selected(0);
        assert_eq!(choice.subtitle(), page_description);
        let (_, query_choice) = build(ExportScope::QueryResult, &result, None, false);
        assert!(query_choice.is_none());
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn query_scope_names_the_page_and_saved_filtered_result() {
        adw::init().unwrap();
        let result = QueryResult {
            columns: Vec::new(),
            rows: vec![Vec::new(), Vec::new()].into(),
            truncated: false,
        };
        let (group, choice) = build(ExportScope::QueryPage { filtered: true }, &result, None, true);
        let choice = choice.unwrap();
        assert_eq!(group.title(), "Scope");
        assert_eq!(choice.selected(), 0);
        assert_eq!(
            choice.subtitle().unwrap().as_str(),
            "2 loaded rows from the current query page"
        );
        choice.set_selected(1);
        assert_eq!(
            choice.subtitle().unwrap().as_str(),
            "Every row in the filtered result, read from saved query data"
        );
    }
}
