# PostgreSQL driver coverage

Measured on 2026-10-24 at Linux head `d2166fdf025fb3bf0d0ac23a0dfdacddb6508743` with Rust 1.98.1 and cargo-llvm-cov 0.9.1.

The driver library and complete integration target ran with ignored Docker tests included. All 291 tests passed. Line coverage was 3,170 of 3,277 lines (96.7%) across 12 PostgreSQL driver source files. The configured per-crate floor is 84%; the floor checker passed. `transaction.rs` was lowest at 29 of 32 lines (90.6%).

Command:

```bash
TABLEPRO_TEST_RUN_ID=local-issue-782-postgres \
  cargo llvm-cov --no-report --locked -p tablepro-driver-postgres --lib --test integration -- \
  --include-ignored --test-threads=1
cargo llvm-cov report --json --summary-only --output-path target/coverage-postgres.json
python3 scripts/check-crate-coverage-floors.py target/coverage-postgres.json --only crates/drivers/postgres
```

The run used the repository's Docker-backed PostgreSQL fixtures and `scripts/cleanup-testcontainers.py` to remove only containers labeled with the run ID. This is local coverage evidence for the measured revision; other crates and later revisions are not covered by this result.

Recovered from an uncommitted local worktree during the October 26 recovery. The original machine-readable coverage output was not committed with this report, so these historical figures have not been independently reproduced and do not establish current-source coverage.
