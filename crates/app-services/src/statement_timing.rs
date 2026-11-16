#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatementTiming {
    pub run_ms: u128,
    pub approval_wait_ms: u128,
}

impl StatementTiming {
    pub fn new(run_ms: u128, approval_wait_ms: u128) -> Self {
        Self {
            run_ms,
            approval_wait_ms,
        }
    }

    pub fn total(timings: impl IntoIterator<Item = StatementTiming>) -> Self {
        timings.into_iter().fold(Self::default(), |sum, timing| Self {
            run_ms: sum.run_ms.saturating_add(timing.run_ms),
            approval_wait_ms: sum.approval_wait_ms.saturating_add(timing.approval_wait_ms),
        })
    }

    pub fn tooltip(&self) -> String {
        let run = crate::tr!("Run time: {ms} ms").replace("{ms}", &self.run_ms.to_string());
        let scope = crate::tr!(
            "The run time covers sending the statement, server execution, row transfer and decoding. \
             The server does not report its own execution time."
        );
        if self.approval_wait_ms == 0 {
            return format!("{run}\n{scope}");
        }
        let wait = crate::tr!("Approval wait: {ms} ms (not part of the run time)")
            .replace("{ms}", &self.approval_wait_ms.to_string());
        format!("{run}\n{wait}\n{scope}")
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
    fn a_script_adds_up_its_statements() {
        let sum = StatementTiming::total([StatementTiming::new(5, 1), StatementTiming::new(7, 0)]);
        assert_eq!(sum, StatementTiming::new(12, 1));
    }
}
