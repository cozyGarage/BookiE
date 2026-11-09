#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/tests/conformance.rs"]
mod conformance;

use drivers_duckdb::DuckdbDriver;

struct Duckdb;

impl conformance::ConformanceEngine for Duckdb {
    type Driver = DuckdbDriver;

    fn driver() -> DuckdbDriver {
        DuckdbDriver
    }

    const FILE_BASED: bool = true;

    async fn prepare(path: &std::path::Path) {
        duckdb::Connection::open(path).expect("create the database file");
    }
}

conformance_suite!(Duckdb);
