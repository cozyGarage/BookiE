# Workspace unit coverage, October 24, 2026

Source: Linux `583c2441957ad2d719762b12c3e6acb9d0cc181a`.

The nightly workspace unit-coverage command completed locally with Rust 1.98.1 and cargo-llvm-cov 0.9.1:

```sh
CARGO_BUILD_JOBS=8 cargo llvm-cov --workspace --exclude tablepro-driver-duckdb --lib --bins --json --summary-only
```

| Scope | Covered lines | Line coverage |
|---|---:|---:|
| Workspace total | 62,203 / 101,187 | 61.47% |
| `apps/linux/app` | 15,334 / 45,842 | 33.45% |
| `crates/agentd` | 781 / 978 | 79.86% |
| `crates/app-services` | 5,412 / 5,938 | 91.14% |
| `crates/core` | 12,165 / 12,641 | 96.23% |
| `crates/drivers/*` combined | 5,889 / 9,525 | 61.83% |
| `crates/dump` | 365 / 428 | 85.28% |
| `crates/mcp` | 1,745 / 2,636 | 66.20% |
| `crates/policy` | 11,684 / 12,168 | 96.02% |
| `crates/ssh` | 2,685 / 3,620 | 74.17% |
| `crates/storage` | 4,581 / 5,480 | 83.59% |
| `crates/transport` | 1,562 / 1,931 | 80.89% |

The command covers library and binary unit tests. DuckDB is excluded, matching the nightly workflow. These figures do not include driver integration suites, installed-app acceptance, GTK accessibility scenarios, or end-to-end tests. The app percentage is Rust unit-test coverage and does not measure UI interaction coverage.

The current isolated GTK widget inventory contains 164 registered tests. That count includes different kinds of widget-level tests and does not claim 164 end-to-end user scenarios.

Recovered from an uncommitted local worktree during the October 26 recovery. The original machine-readable coverage output was not committed with this report, so these historical figures have not been independently reproduced and do not establish current-source coverage.
