use crate::classify::{StatementClass, StatementFacts};
use crate::effects::Effects;

impl StatementFacts {
    /// One statement that only reads: no writes, session-state changes, host or file
    /// access, or administrative effects. Query cursors accept only these.
    pub fn is_single_read(&self) -> bool {
        let external = Effects::SESSION_STATE
            .union(Effects::HOST_OR_FILE_ACCESS)
            .union(Effects::ADMIN);
        !self.is_multi_statement
            && self.class == StatementClass::Select
            && !self.effects.writes()
            && !self.effects.intersects(external)
    }
}
