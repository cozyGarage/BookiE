pub use bookie_app_services::audit_view::{
    OperationRow, contains_operation, operations, row_needs_attention,
};

use bookie_policy::{AuditApprovalOutcome, AuditOperationClass, AuditTerminalStatus};

pub fn class_label(class: AuditOperationClass) -> String {
    match class {
        AuditOperationClass::Read => crate::tr!("Read"),
        AuditOperationClass::Mutation => crate::tr!("Data change"),
        AuditOperationClass::Ddl => crate::tr!("Schema change"),
        AuditOperationClass::Administrative => crate::tr!("Administrative"),
        AuditOperationClass::TransactionCommit => crate::tr!("Commit"),
        AuditOperationClass::TransactionRollback => crate::tr!("Rollback"),
        AuditOperationClass::UnknownWrite => crate::tr!("Unclassified write"),
    }
}

pub fn approval_label(approval: AuditApprovalOutcome) -> String {
    match approval {
        AuditApprovalOutcome::NotRequired => crate::tr!("no approval needed"),
        AuditApprovalOutcome::Approved => crate::tr!("approved"),
        AuditApprovalOutcome::Denied => crate::tr!("denied"),
        AuditApprovalOutcome::Cancelled => crate::tr!("approval cancelled"),
        AuditApprovalOutcome::TimedOut => crate::tr!("approval timed out"),
    }
}

pub fn status_label(status: AuditTerminalStatus) -> String {
    match status {
        AuditTerminalStatus::Pending => crate::tr!("No outcome recorded"),
        AuditTerminalStatus::Succeeded => crate::tr!("Succeeded"),
        AuditTerminalStatus::Failed => crate::tr!("Failed"),
        AuditTerminalStatus::Denied => crate::tr!("Denied"),
        AuditTerminalStatus::Cancelled => crate::tr!("Cancelled"),
        AuditTerminalStatus::TimedOut => crate::tr!("Timed out"),
        AuditTerminalStatus::Unknown => crate::tr!("Unknown outcome"),
    }
}

pub fn matches(row: &OperationRow, query: &str) -> bool {
    bookie_app_services::audit_view::matches(row, query, &class_label(row.class), &status_label(row.status))
}
