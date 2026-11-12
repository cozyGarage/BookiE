use std::rc::Rc;

use bookie_app_services::value_search::{SearchReport, TableOutcome, open_filter};
use bookie_core::{FilterSet, SearchMode};
use relm4::adw::prelude::*;
use relm4::{adw, gtk};

pub(super) type OpenHit = Rc<dyn Fn(Option<String>, String, FilterSet)>;

pub(super) fn note_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .wrap(true)
        .halign(gtk::Align::Start)
        .css_classes(["dim-label"])
        .build()
}

pub(super) fn fill(container: &gtk::Box, report: &SearchReport, term: &str, mode: SearchMode, open: &OpenHit) {
    let hits: Vec<&TableOutcome> = report.with_hits().collect();
    let searched = report.tables.len();
    container.append(&note_label(&summary_text(searched, hits.len())));
    if !hits.is_empty() {
        let group = adw::PreferencesGroup::builder()
            .title(bookie_app_services::tr!("Tables with matches"))
            .build();
        for outcome in hits {
            group.add(&table_row(outcome, term, mode, open));
        }
        container.append(&group);
    }
    for note in report.notes() {
        container.append(&note_label(&note));
    }
}

pub(super) fn summary_text(searched: usize, with_hits: usize) -> String {
    if with_hits == 0 {
        return bookie_app_services::tr!("No matches in {searched} tables.")
            .replace("{searched}", &searched.to_string());
    }
    bookie_app_services::tr!("{hits} of {searched} tables have matches.")
        .replace("{hits}", &with_hits.to_string())
        .replace("{searched}", &searched.to_string())
}

pub(super) fn row_subtitle(outcome: &TableOutcome) -> String {
    let template = if outcome.more_rows_exist {
        bookie_app_services::tr!("{count} or more matching rows in {columns}")
    } else {
        bookie_app_services::tr!("{count} matching rows in {columns}")
    };
    template
        .replace("{count}", &outcome.hits.len().to_string())
        .replace("{columns}", &outcome.matched_columns.join(", "))
}

fn table_row(outcome: &TableOutcome, term: &str, mode: SearchMode, open: &OpenHit) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .use_markup(false)
        .title(outcome.target.label())
        .subtitle(row_subtitle(outcome))
        .build();
    let button = gtk::Button::builder()
        .label(bookie_app_services::tr!("Open filtered"))
        .valign(gtk::Align::Center)
        .build();
    let filter = open_filter(outcome, term, mode);
    let schema = outcome.target.schema.clone();
    let table = outcome.target.name.clone();
    let open = open.clone();
    button.connect_clicked(move |_| open(schema.clone(), table.clone(), filter.clone()));
    row.add_suffix(&button);
    row
}

#[cfg(test)]
mod tests {
    use bookie_app_services::value_search::{TableStatus, TableTarget};

    use super::*;

    fn outcome(hits: usize, more: bool) -> TableOutcome {
        TableOutcome {
            target: TableTarget {
                schema: Some("public".into()),
                name: "people".into(),
            },
            status: TableStatus::Searched,
            hits: (0..hits)
                .map(|_| bookie_app_services::value_search::TableHit {
                    shown: Vec::new(),
                    matched: vec!["name".into()],
                })
                .collect(),
            matched_columns: vec!["name".into(), "bio".into()],
            more_rows_exist: more,
            columns_not_searched: 0,
        }
    }

    #[test]
    fn the_row_says_how_many_rows_and_which_columns_matched() {
        assert_eq!(row_subtitle(&outcome(3, false)), "3 matching rows in name, bio");
        assert_eq!(
            row_subtitle(&outcome(25, true)),
            "25 or more matching rows in name, bio"
        );
    }

    #[test]
    fn the_summary_counts_searched_tables_and_tables_with_matches() {
        assert_eq!(summary_text(12, 0), "No matches in 12 tables.");
        assert_eq!(summary_text(12, 2), "2 of 12 tables have matches.");
    }
}
