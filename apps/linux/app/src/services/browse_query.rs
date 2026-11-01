pub(crate) use bookie_app_services::browse_query::{BoundQuery, BrowseTarget, PageQuery, mark_partial_cells};

#[cfg(test)]
#[path = "browse_query_tests.rs"]
mod tests;
