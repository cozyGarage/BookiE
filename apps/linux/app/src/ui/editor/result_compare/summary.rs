use bookie_app_services::result_compare::{CompareError, CompareReport, CompareSummary};

use crate::ui::count_format::format_thousands;

fn count(label: String, value: u64) -> String {
    format!("{label} {}", format_thousands(u128::from(value)))
}

pub(super) fn differing_rows(summary: &CompareSummary) -> u64 {
    summary
        .added
        .saturating_add(summary.removed)
        .saturating_add(summary.changed)
}

pub(super) fn summary_text(summary: &CompareSummary) -> String {
    let mut parts = vec![
        count(crate::tr!("Equal"), summary.equal),
        count(crate::tr!("Added"), summary.added),
        count(crate::tr!("Removed"), summary.removed),
    ];
    if summary.key_columns > 0 {
        parts.push(count(crate::tr!("Changed"), summary.changed));
    }
    let unknown = summary.unknown_left.saturating_add(summary.unknown_right);
    if unknown > 0 {
        parts.push(count(crate::tr!("Not comparable"), unknown));
    }
    parts.join(" · ")
}

pub(super) fn notes(report: &CompareReport) -> Vec<String> {
    let mut notes = Vec::new();
    if report.summary.differences_capped {
        notes.push(crate::tr!(
            "The list of differences is capped; the counts are complete."
        ));
    }
    if !report.columns_only_left.is_empty() || !report.columns_only_right.is_empty() {
        notes.push(crate::tr!("Columns that are not in both results were not compared."));
    }
    notes
}

pub(super) fn no_difference_message(report: &CompareReport) -> String {
    let mut message = crate::tr!("No differences · {summary}").replace("{summary}", &summary_text(&report.summary));
    for note in notes(report) {
        message = format!("{message} · {note}");
    }
    message
}

pub(super) fn failure_message(error: &CompareError) -> String {
    match error {
        CompareError::Cancelled => crate::tr!("Comparison cancelled"),
        CompareError::IncompleteInput => crate::tr!("A result is incomplete, so it cannot be compared"),
        CompareError::NoCommonColumns => crate::tr!("The results share no column names"),
        CompareError::AmbiguousColumns => crate::tr!("A result has two columns with the same name"),
        CompareError::UnknownKeyColumn => crate::tr!("The key column is not in both results"),
        CompareError::UnreadableKey => crate::tr!("A key value is NULL or could not be read"),
        CompareError::DuplicateKey => crate::tr!("A key value appears more than once; choose a unique key"),
        CompareError::TooManyRows => crate::tr!("The results are too large to compare"),
        CompareError::Source => crate::tr!("A saved result could not be read"),
    }
}
