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
        count(bookie_app_services::tr!("Equal"), summary.equal),
        count(bookie_app_services::tr!("Added"), summary.added),
        count(bookie_app_services::tr!("Removed"), summary.removed),
    ];
    if summary.key_columns > 0 {
        parts.push(count(bookie_app_services::tr!("Changed"), summary.changed));
    }
    let unknown = summary.unknown_left.saturating_add(summary.unknown_right);
    if unknown > 0 {
        parts.push(count(bookie_app_services::tr!("Not comparable"), unknown));
    }
    parts.join(" · ")
}

pub(super) fn notes(report: &CompareReport) -> Vec<String> {
    let mut notes = Vec::new();
    if report.summary.differences_capped {
        notes.push(bookie_app_services::tr!(
            "The list of differences is capped; the counts are complete."
        ));
    }
    if !report.columns_only_left.is_empty() || !report.columns_only_right.is_empty() {
        notes.push(bookie_app_services::tr!(
            "Columns that are not in both results were not compared."
        ));
    }
    notes
}

pub(super) fn no_difference_message(report: &CompareReport) -> String {
    let mut message =
        bookie_app_services::tr!("No differences · {summary}").replace("{summary}", &summary_text(&report.summary));
    for note in notes(report) {
        message = format!("{message} · {note}");
    }
    message
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    Identical,
    TooLargeToShow,
    Show,
}

pub(super) fn verdict(report: &CompareReport) -> Verdict {
    let unknown = report.summary.unknown_left.saturating_add(report.summary.unknown_right);
    if !report.diff.rows.is_empty() {
        return Verdict::Show;
    }
    if differing_rows(&report.summary) > 0 {
        return Verdict::TooLargeToShow;
    }
    if unknown == 0 {
        Verdict::Identical
    } else {
        Verdict::Show
    }
}

pub(super) fn too_large_message(report: &CompareReport) -> String {
    let mut message = bookie_app_services::tr!("Differences found, but they are too large to show · {summary}")
        .replace("{summary}", &summary_text(&report.summary));
    for note in notes(report) {
        message = format!("{message} · {note}");
    }
    message
}

pub(super) fn failure_message(error: &CompareError) -> String {
    match error {
        CompareError::Cancelled => bookie_app_services::tr!("Comparison cancelled"),
        CompareError::IncompleteInput => bookie_app_services::tr!("A result is incomplete, so it cannot be compared"),
        CompareError::NoCommonColumns => bookie_app_services::tr!("The results share no column names"),
        CompareError::AmbiguousColumns => bookie_app_services::tr!("A result has two columns with the same name"),
        CompareError::UnknownKeyColumn => bookie_app_services::tr!("The key column is not in both results"),
        CompareError::UnreadableKey => bookie_app_services::tr!("A key value is NULL or could not be read"),
        CompareError::DuplicateKey => {
            bookie_app_services::tr!("A key value appears more than once; choose a unique key")
        }
        CompareError::TooManyRows => bookie_app_services::tr!("The results are too large to compare"),
        CompareError::Source => bookie_app_services::tr!("A saved result could not be read"),
    }
}
