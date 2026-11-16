use std::sync::{Arc, OnceLock};

use bookie_policy::ApprovalGrants;

static GRANTS: OnceLock<Arc<ApprovalGrants>> = OnceLock::new();

pub fn shared() -> Arc<ApprovalGrants> {
    GRANTS.get_or_init(Arc::default).clone()
}
