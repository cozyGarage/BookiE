use sha2::{Digest, Sha256};

use super::super::StatementIdentity;

pub(super) fn statement_identity(statement_index: usize, sql: &str) -> StatementIdentity {
    let mut digest = Sha256::new();
    digest.update((statement_index as u64).to_be_bytes());
    digest.update((sql.len() as u64).to_be_bytes());
    digest.update(sql.as_bytes());
    StatementIdentity {
        statement_index,
        digest: digest.finalize().into(),
    }
}

pub(super) fn sql_preview(sql: &str) -> String {
    let single_line: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() > 60 {
        let prefix: String = single_line.chars().take(60).collect();
        format!("{prefix}…")
    } else {
        single_line
    }
}
