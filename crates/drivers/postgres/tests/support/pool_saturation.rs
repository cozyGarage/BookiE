#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{connect, start_pg};

#[macro_use]
#[path = "../../../shared/pool_saturation.rs"]
mod contract;

pool_saturation_tests!(start_pg, "SELECT pg_sleep(12)");
