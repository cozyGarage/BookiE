#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatementTiming {
    pub run_ms: u128,
    pub approval_wait_ms: u128,
    pub first_row_ms: Option<u128>,
}

impl StatementTiming {
    pub fn new(run_ms: u128, approval_wait_ms: u128) -> Self {
        Self {
            run_ms,
            approval_wait_ms,
            first_row_ms: None,
        }
    }

    pub fn with_first_row(mut self, first_row_ms: Option<u128>) -> Self {
        self.first_row_ms = first_row_ms;
        self
    }

    pub fn total(timings: impl IntoIterator<Item = StatementTiming>) -> Self {
        let mut total = Self::default();
        let mut first_row_ms = Some(0u128);
        let mut count = 0;
        for timing in timings {
            total.run_ms = total.run_ms.saturating_add(timing.run_ms);
            total.approval_wait_ms = total.approval_wait_ms.saturating_add(timing.approval_wait_ms);
            first_row_ms = first_row_ms.and_then(|sum| timing.first_row_ms.map(|value| sum.saturating_add(value)));
            count += 1;
        }
        total.first_row_ms = (count > 0).then_some(first_row_ms).flatten();
        total
    }

    pub fn tooltip(&self) -> String {
        let run = crate::tr!("Run time: {ms} ms").replace("{ms}", &self.run_ms.to_string());
        let first_row =
            self.first_row_ms
                .filter(|elapsed| *elapsed <= self.run_ms)
                .map_or_else(String::new, |elapsed| {
                    let first_row =
                        crate::tr!("Time to first row or completion: {ms} ms").replace("{ms}", &elapsed.to_string());
                    let transfer = crate::tr!("Result transfer: {ms} ms")
                        .replace("{ms}", &self.run_ms.saturating_sub(elapsed).to_string());
                    format!("{first_row}\n{transfer}\n")
                });
        let scope = crate::tr!(
            "The run time covers sending the statement, server execution, row transfer and decoding. \
             The server does not report its own execution time."
        );
        if self.approval_wait_ms == 0 {
            return format!("{run}\n{first_row}{scope}");
        }
        let wait = crate::tr!("Approval wait: {ms} ms (not part of the run time)")
            .replace("{ms}", &self.approval_wait_ms.to_string());
        format!("{run}\n{first_row}{wait}\n{scope}")
    }
}

#[cfg(test)]
mod tests {
    use super::StatementTiming;

    #[test]
    fn the_tooltip_reports_the_run_time_and_says_the_server_time_is_unknown() {
        let tooltip = StatementTiming::new(42, 0).tooltip();
        assert!(tooltip.contains("42"), "{tooltip}");
        assert!(tooltip.contains("does not report"), "{tooltip}");
        assert!(!tooltip.contains("Approval"), "{tooltip}");
    }

    #[test]
    fn the_tooltip_lists_the_approval_wait_only_when_there_was_one() {
        let tooltip = StatementTiming::new(10, 2500).tooltip();
        assert!(tooltip.contains("2500"), "{tooltip}");
        assert!(tooltip.contains("Approval wait"), "{tooltip}");
    }

    #[test]
    fn the_tooltip_reports_first_row_and_transfer_only_when_observed() {
        let timing = StatementTiming::new(90, 0).with_first_row(Some(30));
        assert_eq!(timing.first_row_ms, Some(30));
        let observed = timing.tooltip();
        assert!(
            observed.contains("Time to first row or completion: 30 ms"),
            "{observed}"
        );
        assert!(observed.contains("Result transfer: 60 ms"), "{observed}");

        let buffered = StatementTiming::new(90, 0).tooltip();
        assert!(!buffered.contains("first row"), "{buffered}");
        assert!(!buffered.contains("Result transfer"), "{buffered}");

        let inconsistent = StatementTiming::new(10, 0).with_first_row(Some(11)).tooltip();
        assert!(!inconsistent.contains("first row"), "{inconsistent}");
    }

    #[test]
    fn a_script_shows_first_row_timing_only_when_every_statement_has_it() {
        let streamed = StatementTiming::total([
            StatementTiming::new(50, 0).with_first_row(Some(20)),
            StatementTiming::new(40, 0).with_first_row(Some(10)),
        ]);
        assert_eq!(streamed.first_row_ms, Some(30));

        let mixed = StatementTiming::total([
            StatementTiming::new(50, 0).with_first_row(Some(20)),
            StatementTiming::new(40, 0),
        ]);
        assert_eq!(mixed.first_row_ms, None);
    }

    #[test]
    fn a_script_adds_up_its_statements() {
        let sum = StatementTiming::total([StatementTiming::new(5, 1), StatementTiming::new(7, 0)]);
        assert_eq!(sum, StatementTiming::new(12, 1));
    }
}
