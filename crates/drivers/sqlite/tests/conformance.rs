#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../shared/tests/conformance.rs"]
mod conformance;

use drivers_sqlite::SqliteDriver;

struct Sqlite;

impl conformance::ConformanceEngine for Sqlite {
    type Driver = SqliteDriver;

    fn driver() -> SqliteDriver {
        SqliteDriver
    }

    const FILE_BASED: bool = true;

    fn prepare(path: &std::path::Path) {
        std::fs::File::create(path).expect("create the database file");
    }
}

conformance_suite!(Sqlite);
