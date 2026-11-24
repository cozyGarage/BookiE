use std::time::Duration;

pub const IDLE_WARNING_AFTER: Duration = Duration::from_secs(5 * 60);

pub fn idle_warning(idle_for: Duration) -> Option<String> {
    if idle_for < IDLE_WARNING_AFTER {
        return None;
    }
    let minutes = idle_for.as_secs() / 60;
    Some(
        crate::tr!("Idle for {minutes} min: the open transaction may hold locks. Commit or roll back.")
            .replace("{minutes}", &minutes.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_warning_starts_at_five_idle_minutes_and_counts_whole_minutes() {
        assert_eq!(idle_warning(Duration::from_secs(299)), None);
        let warning = idle_warning(Duration::from_secs(300)).expect("a warning at five minutes");
        assert!(warning.starts_with("Idle for 5 min"), "{warning}");
        assert!(
            idle_warning(Duration::from_secs(61 * 60))
                .expect("warning")
                .contains("61 min")
        );
    }
}
