use std::collections::BTreeMap;

use crate::corpus::{Corpus, KnownFailing, Route};

#[derive(Debug, Clone)]
pub enum Outcome {
    Pass { rows: usize },
    Fail(Vec<String>),
    Skipped(String),
    Broken(String),
}

#[derive(Debug, Clone)]
pub struct CaseResult {
    pub server: String,
    pub case: String,
    pub route: Route,
    pub outcome: Outcome,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Tally {
    pub pass: usize,
    pub known_failing: usize,
    pub skipped: usize,
    pub failed: usize,
    pub fixed: usize,
}

#[derive(Debug, Default)]
pub struct Verdict {
    pub tallies: BTreeMap<String, Tally>,
    pub known: Vec<String>,
    pub problems: Vec<String>,
}

pub fn judge(corpus: &Corpus, results: &[CaseResult]) -> Verdict {
    judge_with(results, |result| {
        corpus
            .cases
            .iter()
            .find(|case| case.id == result.case)
            .unwrap_or_else(|| panic!("result for unknown case {}", result.case))
            .known_issue(result.route, &result.server)
    })
}

pub fn judge_with<'a>(
    results: &[CaseResult],
    known_issue: impl Fn(&CaseResult) -> Option<&'a KnownFailing>,
) -> Verdict {
    let mut verdict = Verdict::default();
    for result in results {
        let known = known_issue(result);
        let tally = verdict.tallies.entry(result.server.clone()).or_default();
        let label = format!("{} {} {}", result.server, result.case, result.route.name());
        match (&result.outcome, known) {
            (Outcome::Skipped(_), _) => tally.skipped += 1,
            (Outcome::Broken(error), _) => {
                tally.failed += 1;
                verdict.problems.push(format!("{label}: case is broken: {error}"));
            }
            (Outcome::Pass { .. }, None) => tally.pass += 1,
            (Outcome::Pass { .. }, Some(known)) => {
                tally.fixed += 1;
                verdict.problems.push(format!(
                    "{label}: passes now; known issue #{} looks fixed, remove its known_failing marker",
                    known.issue
                ));
            }
            (Outcome::Fail(details), Some(known)) => {
                tally.known_failing += 1;
                verdict
                    .known
                    .push(format!("{label}: still fails as #{} ({})", known.issue, first(details)));
            }
            (Outcome::Fail(details), None) => {
                tally.failed += 1;
                verdict.problems.push(format!("{label}: {}", details.join("; ")));
            }
        }
    }
    verdict
}

fn first(details: &[String]) -> &str {
    details.first().map_or("", String::as_str)
}

impl Verdict {
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        for (server, tally) in &self.tallies {
            lines.push(format!(
                "{server}: {} pass, {} known-failing, {} skipped, {} unexpected failures, {} fixed known issues",
                tally.pass, tally.known_failing, tally.skipped, tally.failed, tally.fixed
            ));
        }
        lines.extend(self.known.iter().cloned());
        lines.join("\n")
    }

    pub fn assert_clean(&self) {
        assert!(
            self.problems.is_empty(),
            "{} data-integrity problems:\n{}\n\nSummary:\n{}",
            self.problems.len(),
            self.problems.join("\n"),
            self.summary()
        );
    }
}
