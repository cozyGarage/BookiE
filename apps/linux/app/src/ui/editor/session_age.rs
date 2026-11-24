use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::{self as gtk, glib, prelude::*};

const REFRESH_SECONDS: u32 = 30;

#[derive(Default)]
struct Inner {
    since: Cell<Option<Instant>>,
    last_activity: Cell<Option<Instant>>,
    timer_running: Cell<bool>,
}

#[derive(Clone, Default)]
pub(super) struct TransactionClock(Rc<Inner>);

impl TransactionClock {
    pub(super) fn set_open(&self, open: bool, now: Instant) {
        match (open, self.0.since.get()) {
            (true, None) => self.0.since.set(Some(now)),
            (false, _) => self.0.since.set(None),
            _ => {}
        }
        self.0.last_activity.set(open.then_some(now));
    }

    pub(super) fn idle_warning_at(&self, now: Instant) -> Option<String> {
        let last = self.0.last_activity.get()?;
        bookie_app_services::transaction_idle::idle_warning(now.saturating_duration_since(last))
    }

    pub(super) fn is_open(&self) -> bool {
        self.0.since.get().is_some()
    }

    pub(super) fn label_at(&self, now: Instant) -> String {
        match self.0.since.get() {
            None => bookie_app_services::tr!("Session"),
            Some(since) => {
                let label = open_label(now.saturating_duration_since(since));
                match self.idle_warning_at(now) {
                    Some(_) => format!("{label} · {}", bookie_app_services::tr!("idle")),
                    None => label,
                }
            }
        }
    }

    pub(super) fn reset(&self, button: &gtk::ToggleButton) {
        self.0.since.set(None);
        self.0.last_activity.set(None);
        button.set_tooltip_text(None);
        button.set_label(&bookie_app_services::tr!("Session"));
    }

    fn paint(&self, button: &gtk::ToggleButton, now: Instant) {
        button.set_label(&self.label_at(now));
        button.set_tooltip_text(self.idle_warning_at(now).as_deref());
    }

    fn claim_timer(&self) -> bool {
        self.0.since.get().is_some() && !self.0.timer_running.replace(true)
    }

    pub(super) fn show(&self, button: &gtk::ToggleButton) {
        self.paint(button, Instant::now());
        if !self.claim_timer() {
            return;
        }
        let (clock, button) = (self.clone(), button.downgrade());
        glib::timeout_add_seconds_local(REFRESH_SECONDS, move || {
            let Some(button) = button.upgrade().filter(|_| clock.0.since.get().is_some()) else {
                clock.0.timer_running.set(false);
                return glib::ControlFlow::Break;
            };
            clock.paint(&button, Instant::now());
            glib::ControlFlow::Continue
        });
    }
}

fn open_label(open_for: Duration) -> String {
    let minutes = open_for.as_secs() / 60;
    if minutes == 0 {
        return bookie_app_services::tr!("Session · transaction open");
    }
    let age = if minutes < 60 {
        minute_label(minutes)
    } else {
        let hours = hour_label(minutes / 60);
        let remainder = minutes % 60;
        if remainder == 0 {
            hours
        } else {
            format!("{hours} {}", minute_label(remainder))
        }
    };
    format!("{} · {age}", bookie_app_services::tr!("Session · transaction open"))
}

fn minute_label(count: u64) -> String {
    bookie_app_services::trn!("{n} minute", "{n} minutes", count).replace("{n}", &count.to_string())
}

fn hour_label(count: u64) -> String {
    bookie_app_services::trn!("{n} hour", "{n} hours", count).replace("{n}", &count.to_string())
}

pub(super) fn retired_session_message(transaction_was_open: bool) -> String {
    if transaction_was_open {
        bookie_app_services::tr!(
            "The connection was lost while a transaction was open. The server discards an uncommitted transaction when its connection ends, so uncommitted changes should not have been saved; check the data before retrying. Reopen Session before running more SQL."
        )
    } else {
        bookie_app_services::tr!(
            "This editor session was retired after a connection failure. Reopen Session before running more SQL."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retired_session_says_so_when_a_transaction_was_open() {
        assert!(retired_session_message(true).contains("transaction was open"));
        assert!(retired_session_message(true).contains("uncommitted changes should not have been saved"));
        assert!(!retired_session_message(false).contains("transaction"));
    }

    #[test]
    fn the_clock_warns_after_five_idle_minutes_and_a_statement_resets_it() {
        let clock = TransactionClock::default();
        let start = Instant::now();
        clock.set_open(true, start);
        assert_eq!(clock.idle_warning_at(start + Duration::from_secs(299)), None);
        assert!(clock.idle_warning_at(start + Duration::from_secs(300)).is_some());
        assert!(clock.label_at(start + Duration::from_secs(300)).ends_with("idle"));
        clock.set_open(true, start + Duration::from_secs(300));
        assert_eq!(clock.idle_warning_at(start + Duration::from_secs(301)), None);
        clock.set_open(false, start + Duration::from_secs(302));
        assert_eq!(clock.idle_warning_at(start + Duration::from_secs(900)), None);
    }

    #[test]
    fn the_clock_knows_whether_a_transaction_is_open() {
        let clock = TransactionClock::default();
        assert!(!clock.is_open());
        clock.set_open(true, Instant::now());
        assert!(clock.is_open());
        clock.set_open(false, Instant::now());
        assert!(!clock.is_open());
    }

    #[test]
    fn the_label_reports_how_long_a_transaction_has_been_open() {
        let clock = TransactionClock::default();
        let start = Instant::now();
        assert_eq!(clock.label_at(start), "Session");
        clock.set_open(true, start);
        assert_eq!(
            clock.label_at(start + Duration::from_secs(59)),
            "Session · transaction open"
        );
        assert_eq!(
            clock.label_at(start + Duration::from_secs(60)),
            "Session · transaction open · 1 minute"
        );
        assert_eq!(
            clock.label_at(start + Duration::from_secs(75 * 60)),
            "Session · transaction open · 1 hour 15 minutes"
        );
        assert_eq!(
            clock.label_at(start + Duration::from_secs(61 * 60)),
            "Session · transaction open · 1 hour 1 minute"
        );
        assert_eq!(
            clock.label_at(start + Duration::from_secs(120 * 60)),
            "Session · transaction open · 2 hours"
        );
    }

    #[test]
    fn a_state_update_while_open_keeps_the_original_start_and_closing_resets_it() {
        let clock = TransactionClock::default();
        let start = Instant::now();
        clock.set_open(true, start);
        clock.set_open(true, start + Duration::from_secs(300));
        assert_eq!(
            clock.label_at(start + Duration::from_secs(300)),
            "Session · transaction open · 5 minutes"
        );
        clock.set_open(false, start + Duration::from_secs(301));
        assert_eq!(clock.label_at(start + Duration::from_secs(400)), "Session");
    }

    #[test]
    fn one_refresh_timer_serves_every_state_update_while_a_transaction_is_open() {
        let clock = TransactionClock::default();
        assert!(!clock.claim_timer(), "no timer without an open transaction");
        clock.set_open(true, Instant::now());
        assert!(clock.claim_timer());
        assert!(!clock.claim_timer(), "a second update must not start a second timer");
        clock.set_open(false, Instant::now());
        clock.set_open(true, Instant::now());
        assert!(
            !clock.claim_timer(),
            "reopening within the interval reuses the running timer"
        );
        clock.0.timer_running.set(false);
        assert!(
            clock.claim_timer(),
            "after the timer stops, the next update may start one"
        );
    }
}
