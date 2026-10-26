# Workspace unit coverage

Measured on 2026-10-24 at Linux head `d2166fdf025fb3bf0d0ac23a0dfdacddb6508743` with Rust 1.98.1 and cargo-llvm-cov 0.9.1. This uses the nightly unit-tier command and excludes DuckDB. The global line coverage was 61.42%; the configured floor is 59% and passed.

| Package | Covered / executable lines | Line coverage |
| --- | ---: | ---: |
| `tablepro-core` | 12,144 / 12,619 | 96.2% |
| `tablepro-policy` | 11,684 / 12,168 | 96.0% |
| `tablepro-storage` | 4,581 / 5,480 | 83.6% |
| `tablepro-ssh` | 2,685 / 3,620 | 74.2% |
| `tablepro-transport` | 1,562 / 1,931 | 80.9% |
| `tablepro-mcp` | 1,745 / 2,636 | 66.2% |
| `tablepro-agentd` | 781 / 978 | 79.9% |
| `tablepro-app-services` | 5,411 / 5,938 | 91.1% |
| PostgreSQL driver | 1,958 / 3,277 | 59.7% |
| MySQL driver | 269 / 915 | 29.4% |
| SQLite driver | 620 / 788 | 78.7% |
| MongoDB driver | 1,680 / 2,152 | 78.1% |
| Redis driver | 371 / 609 | 60.9% |
| ClickHouse driver | 497 / 849 | 58.5% |
| SQL Server driver | 494 / 935 | 52.8% |
| `tablepro-dump` | 312 / 374 | 83.4% |
| `tablepro-app` | 15,288 / 45,802 | 33.4% |

Command:

```bash
cargo llvm-cov --workspace --exclude tablepro-driver-duckdb --lib --bins \
  --json --summary-only --output-path target/coverage-unit-20261024.json
```

These are unit-tier figures only. Driver Docker suites are measured separately; for example, the PostgreSQL driver reaches 96.7% with its complete integration suite in the [driver report](../postgres-driver-coverage-2026-10-24/README.md). The app figure does not measure GTK scenario, installed-app or end-to-end behavior.

Recovered from an uncommitted local worktree during the October 26 recovery. The original machine-readable coverage output was not committed with this report, so these historical figures have not been independently reproduced and do not establish current-source coverage.
