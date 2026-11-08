pub use crate::ordered_refresh::after_mutation;
pub use crate::request_generation::{
    Generation, LatestApplied, RevisionedGeneration, RevisionedTicket, is_current, replace_in_flight,
    replace_scoped_in_flight,
};
pub use crate::single_flight::{Flight, SingleFlight};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_surface_drops_stale_exact_and_ordered_replies() {
        let exact = Generation::default();
        let older = exact.begin();
        let newer = exact.begin();
        assert!(!exact.accepts(older));
        assert!(exact.accepts(newer));
        assert!(is_current(newer, exact.current()));

        let order = LatestApplied::default();
        let first = order.begin();
        let second = order.begin();
        assert!(order.accept(second));
        assert!(!order.accept(first));
    }

    #[test]
    fn the_shared_surface_refuses_a_second_flight_until_the_first_ends() {
        let gate = SingleFlight::default();
        let first = gate.try_begin().expect("idle gate admits an operation");
        assert!(gate.try_begin().is_none());
        drop(first);
        assert!(gate.try_begin().is_some());
    }
}
