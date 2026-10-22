use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::{self as gtk, glib, prelude::*};

const REFRESH_SECONDS: u32 = 30;

#[derive(Clone, Default)]
pub(super) struct TransactionClock(Rc<Cell<Option<Instant>>>);

impl TransactionClock {
    pub(super) fn set_open(&self, open: bool, now: Instant) {
        match (open, self.0.get()) {
            (true, None) => self.0.set(Some(now)),
            (false, _) => self.0.set(None),
            _ => {}
        }
    }

    pub(super) fn label_at(&self, now: Instant) -> String {
        match self.0.get() {
            None => crate::tr!("Session"),
            Some(since) => open_label(now.saturating_duration_since(since)),
        }
    }

    pub(super) fn reset(&self, button: &gtk::ToggleButton) {
        self.0.set(None);
        button.set_label(&crate::tr!("Session"));
    }

    pub(super) fn show(&self, button: &gtk::ToggleButton) {
        button.set_label(&self.label_at(Instant::now()));
        if self.0.get().is_none() {
            return;
        }
        let (clock, button) = (self.clone(), button.downgrade());
        glib::timeout_add_seconds_local(REFRESH_SECONDS, move || {
            let Some(button) = button.upgrade().filter(|_| clock.0.get().is_some()) else {
                return glib::ControlFlow::Break;
            };
            button.set_label(&clock.label_at(Instant::now()));
            glib::ControlFlow::Continue
        });
    }
}

fn open_label(open_for: Duration) -> String {
    let minutes = open_for.as_secs() / 60;
    if minutes == 0 {
        return crate::tr!("Session · transaction open");
    }
    let age = if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h {} min", minutes / 60, minutes % 60)
    };
    format!("{} · {age}", crate::tr!("Session · transaction open"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "Session · transaction open · 1 min"
        );
        assert_eq!(
            clock.label_at(start + Duration::from_secs(75 * 60)),
            "Session · transaction open · 1 h 15 min"
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
            "Session · transaction open · 5 min"
        );
        clock.set_open(false, start + Duration::from_secs(301));
        assert_eq!(clock.label_at(start + Duration::from_secs(400)), "Session");
        clock.set_open(true, start + Duration::from_secs(500));
        clock.0.set(None);
        assert_eq!(clock.label_at(start + Duration::from_secs(900)), "Session");
    }
}
