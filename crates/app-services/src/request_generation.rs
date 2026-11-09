pub fn is_current(scheduled: u64, current: u64) -> bool {
    scheduled == current
}

pub fn replace_in_flight(
    current: &std::cell::RefCell<tokio_util::sync::CancellationToken>,
) -> tokio_util::sync::CancellationToken {
    current.borrow().cancel();
    let next = tokio_util::sync::CancellationToken::new();
    *current.borrow_mut() = next.clone();
    next
}

pub fn replace_scoped_in_flight(
    current: &std::cell::RefCell<tokio_util::sync::CancellationToken>,
    owner: &tokio_util::sync::CancellationToken,
) -> tokio_util::sync::CancellationToken {
    current.borrow().cancel();
    let next = owner.child_token();
    *current.borrow_mut() = next.clone();
    next
}

#[derive(Debug, Default)]
pub struct LatestApplied {
    issued: std::cell::Cell<u64>,
    applied: std::cell::Cell<u64>,
}

impl LatestApplied {
    pub fn begin(&self) -> u64 {
        let ticket = self.issued.get() + 1;
        self.issued.set(ticket);
        ticket
    }

    pub fn accept(&self, ticket: u64) -> bool {
        if ticket < self.applied.get() {
            return false;
        }
        self.applied.set(ticket);
        true
    }
}

#[derive(Debug, Default)]
pub struct Generation {
    current: std::cell::Cell<u64>,
}

impl Generation {
    pub fn begin(&self) -> u64 {
        let ticket = self.current.get().wrapping_add(1);
        self.current.set(ticket);
        ticket
    }

    pub fn current(&self) -> u64 {
        self.current.get()
    }

    pub fn accepts(&self, ticket: u64) -> bool {
        is_current(ticket, self.current.get())
    }

    pub fn invalidate(&self) {
        let _ = self.begin();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionedTicket {
    generation: u64,
    revision: u64,
}

impl RevisionedTicket {
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

#[derive(Debug, Default)]
pub struct RevisionedGeneration {
    generation: std::cell::Cell<u64>,
    revision: std::cell::Cell<u64>,
}

impl RevisionedGeneration {
    pub fn revision(&self) -> u64 {
        self.revision.get()
    }

    pub fn changed(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
    }

    pub fn begin(&self) -> RevisionedTicket {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        RevisionedTicket {
            generation,
            revision: self.revision.get(),
        }
    }

    pub fn accepts(&self, ticket: RevisionedTicket) -> bool {
        self.generation.get() == ticket.generation && self.revision.get() == ticket.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_generation_must_not_apply() {
        assert!(!is_current(1, 2));
        assert!(!is_current(0, 1));
    }

    #[test]
    fn replacing_the_in_flight_token_cancels_the_previous_one_only() {
        let slot = std::cell::RefCell::new(tokio_util::sync::CancellationToken::new());
        let first = slot.borrow().clone();
        let second = replace_in_flight(&slot);
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        let third = replace_in_flight(&slot);
        assert!(second.is_cancelled());
        assert!(!third.is_cancelled());
    }

    #[test]
    fn replacing_a_scoped_request_and_retiring_its_owner_cancel_only_that_scope() {
        let slot = std::cell::RefCell::new(tokio_util::sync::CancellationToken::new());
        let owner = tokio_util::sync::CancellationToken::new();
        let first = replace_scoped_in_flight(&slot, &owner);
        let second = replace_scoped_in_flight(&slot, &owner);
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        owner.cancel();
        assert!(second.is_cancelled());
        let next_owner = tokio_util::sync::CancellationToken::new();
        let next = replace_scoped_in_flight(&slot, &next_owner);
        assert!(!next.is_cancelled());
    }

    #[test]
    fn a_matching_generation_applies() {
        assert!(is_current(0, 0));
        assert!(is_current(3, 3));
    }

    #[test]
    fn tickets_grow_with_every_request() {
        let order = LatestApplied::default();
        assert_eq!(order.begin(), 1);
        assert_eq!(order.begin(), 2);
        assert_eq!(order.begin(), 3);
    }

    #[test]
    fn a_slow_older_request_cannot_overwrite_a_newer_result() {
        let order = LatestApplied::default();
        let older = order.begin();
        let newer = order.begin();
        assert!(order.accept(newer));
        assert!(!order.accept(older));
    }

    #[test]
    fn results_arriving_in_request_order_all_apply() {
        let order = LatestApplied::default();
        let first = order.begin();
        let second = order.begin();
        assert!(order.accept(first));
        assert!(order.accept(second));
    }

    #[test]
    fn a_dropped_older_request_does_not_block_the_newer_one() {
        let order = LatestApplied::default();
        let _lost = order.begin();
        let kept = order.begin();
        assert!(order.accept(kept));
    }

    #[test]
    fn a_repeated_result_for_the_same_request_still_applies() {
        let order = LatestApplied::default();
        let ticket = order.begin();
        assert!(order.accept(ticket));
        assert!(order.accept(ticket));
    }

    #[test]
    fn an_exact_generation_rejects_every_older_ticket() {
        let gate = Generation::default();
        let first = gate.begin();
        let second = gate.begin();
        assert!(!gate.accepts(first));
        assert!(gate.accepts(second));
        assert_eq!(gate.current(), second);
    }

    #[test]
    fn invalidating_an_exact_generation_drops_the_pending_ticket() {
        let gate = Generation::default();
        let pending = gate.begin();
        gate.invalidate();
        assert!(!gate.accepts(pending));
        let next = gate.begin();
        assert!(gate.accepts(next));
    }

    #[test]
    fn a_revision_bump_rejects_an_otherwise_current_ticket() {
        let gate = RevisionedGeneration::default();
        let first = gate.begin();
        let second = gate.begin();
        assert!(!gate.accepts(first));
        assert!(gate.accepts(second));
        gate.changed();
        assert!(!gate.accepts(second));
        let resumed = gate.begin();
        assert!(gate.accepts(resumed));
        assert_eq!(gate.revision(), resumed.revision());
    }
}
