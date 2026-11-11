use bookie_policy::SkippedRow;

const KEPT_ROWS: usize = 100;
const SHOWN_ROWS: usize = 20;

#[derive(Debug, Default)]
pub(super) struct SkipLog {
    total: u64,
    kept: Vec<(u64, String)>,
}

impl SkipLog {
    pub(super) fn record(&mut self, rows_before_batch: u64, skipped: &[SkippedRow]) {
        for row in skipped {
            self.total += 1;
            if self.kept.len() < KEPT_ROWS {
                self.kept
                    .push((rows_before_batch + row.index as u64 + 1, row.reason.clone()));
            }
        }
    }

    pub(super) fn total(&self) -> u64 {
        self.total
    }

    pub(super) fn summary(&self) -> String {
        let mut lines: Vec<String> = self
            .kept
            .iter()
            .take(SHOWN_ROWS)
            .map(|(row, reason)| {
                bookie_app_services::tr!("Row {row}: {reason}")
                    .replace("{row}", &row.to_string())
                    .replace("{reason}", reason)
            })
            .collect();
        let hidden = self.total.saturating_sub(lines.len() as u64);
        if hidden > 0 {
            lines.push(bookie_app_services::tr!("…and {n} more skipped rows.").replace("{n}", &hidden.to_string()));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skipped(index: usize) -> SkippedRow {
        SkippedRow {
            index,
            reason: format!("reason {index}"),
        }
    }

    #[test]
    fn rows_are_numbered_across_batches_counting_from_one() {
        let mut log = SkipLog::default();
        log.record(0, &[skipped(1)]);
        log.record(500, &[skipped(0), skipped(7)]);
        assert_eq!(log.total(), 3);
        let summary = log.summary();
        assert_eq!(
            summary.lines().collect::<Vec<_>>(),
            ["Row 2: reason 1", "Row 501: reason 0", "Row 508: reason 7"]
        );
    }

    #[test]
    fn the_report_is_bounded_but_the_total_stays_exact() {
        let mut log = SkipLog::default();
        let many: Vec<SkippedRow> = (0..300).map(skipped).collect();
        log.record(0, &many);
        assert_eq!(log.total(), 300);
        assert_eq!(log.kept.len(), KEPT_ROWS);
        let summary = log.summary();
        assert_eq!(summary.lines().count(), SHOWN_ROWS + 1);
        assert!(summary.ends_with("…and 280 more skipped rows."), "{summary}");
    }
}
