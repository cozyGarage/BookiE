pub use bookie_app_services::audit_view::{OperationRow, contains_operation, operations, row_needs_attention};

use bookie_policy::{AuditApprovalOutcome, AuditOperationClass, AuditTerminalStatus};

pub fn class_label(class: AuditOperationClass) -> String {
    match class {
        AuditOperationClass::Read => bookie_app_services::tr!("Read"),
        AuditOperationClass::Mutation => bookie_app_services::tr!("Data change"),
        AuditOperationClass::Ddl => bookie_app_services::tr!("Schema change"),
        AuditOperationClass::Administrative => bookie_app_services::tr!("Administrative"),
        AuditOperationClass::TransactionCommit => bookie_app_services::tr!("Commit"),
        AuditOperationClass::TransactionRollback => bookie_app_services::tr!("Rollback"),
        AuditOperationClass::UnknownWrite => bookie_app_services::tr!("Unclassified write"),
    }
}

pub fn approval_label(approval: AuditApprovalOutcome) -> String {
    match approval {
        AuditApprovalOutcome::NotRequired => bookie_app_services::tr!("no approval needed"),
        AuditApprovalOutcome::Approved => bookie_app_services::tr!("approved"),
        AuditApprovalOutcome::Denied => bookie_app_services::tr!("denied"),
        AuditApprovalOutcome::Cancelled => bookie_app_services::tr!("approval cancelled"),
        AuditApprovalOutcome::TimedOut => bookie_app_services::tr!("approval timed out"),
    }
}

pub fn status_label(status: AuditTerminalStatus) -> String {
    match status {
        AuditTerminalStatus::Pending => bookie_app_services::tr!("No outcome recorded"),
        AuditTerminalStatus::Succeeded => bookie_app_services::tr!("Succeeded"),
        AuditTerminalStatus::Failed => bookie_app_services::tr!("Failed"),
        AuditTerminalStatus::Denied => bookie_app_services::tr!("Denied"),
        AuditTerminalStatus::Cancelled => bookie_app_services::tr!("Cancelled"),
        AuditTerminalStatus::TimedOut => bookie_app_services::tr!("Timed out"),
        AuditTerminalStatus::Unknown => bookie_app_services::tr!("Unknown outcome"),
    }
}

pub fn matches(row: &OperationRow, query: &str) -> bool {
    bookie_app_services::audit_view::matches(row, query, &class_label(row.class), &status_label(row.status))
}
