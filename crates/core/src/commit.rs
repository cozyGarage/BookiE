#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitToken {
    PostgresXid { xid: u64, legacy_txid: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommitResolution {
    Applied,
    NotApplied,
    StillRunning,
    Unresolvable,
}
