# UI and release verification, 2026-10-26

Source: GitHub `linux` at `b0f1ed02f42b2d166b2a437f4648b605a6fe331e`, including merged PRs [#1682](https://github.com/cozyGarage/BookiE/pull/1682) and [#1685](https://github.com/cozyGarage/BookiE/pull/1685).

The app was rebuilt from that source with `cargo build --locked -p tablepro-app --bin tablepro-app`. The binary SHA-256 was `032fec0bc3af85bc8097f90120550b5e5483fb0bc0002c3e57264759dee59f52`.

## UI evidence

- `postgres_completion_uses_each_editor_session_search_path` passed against the PostgreSQL 17 fixture in 21.399 seconds using the rebuilt binary. It verified that separate editor sessions returned completion columns from their own `search_path`.
- The full installed GTK safety suite passed under Xvfb, AT-SPI, and a private D-Bus session.
- The PostgreSQL release GTK scenarios passed, including mTLS, session cleanup, host-key refusal and trust, and multi-hop SSH.
- Installed GTK scenarios passed for ClickHouse Enum16 editing, MySQL approval, and MongoDB cursor/edit behavior.
- [Scenario result](artifacts/postgres_completion_uses_each_editor_session_search_path-result.json) records the completion UI pass. UI automation ran under Xvfb; native Wayland interaction was not exercised.

## Automated test tiers

- `scripts/preflight.sh`: 2,170 unit/bin tests passed (16 skipped), 291 sandbox tests passed (50 skipped), and the Redis cancellation test passed. Formatting, Clippy, and repository guards passed.
- Full workspace fast checks: 2,975 tests passed (229 skipped); 291 sandbox tests and one Redis cancellation test passed. The 174 isolated GTK widget tests passed.
- Docker driver integration passed: ClickHouse 39, MongoDB 54, SQL Server 45, MySQL/MariaDB 134, PostgreSQL 294, and Redis 14. MCP MongoDB 1, MCP PostgreSQL enum 3, policy PostgreSQL 12, policy SQL Server 2, PostgreSQL socket 1, and SSH 23 tests also passed.
- Driver TLS passed for ClickHouse, MongoDB, SQL Server, MySQL, Redis, plaintext refusal, and system trust. Secret Service keyring tests passed.
- PostgreSQL release tests and their installed GTK scenarios passed.
- DuckDB passed 67 tests, and `cargo build --locked -p tablepro-app --features duckdb` passed.
- `cargo deny check` passed. `cargo audit` completed with four allowed warnings: three unmaintained `zbus` family crates and one yanked `crypto-bigint` version.

The first `ci-local.sh release` attempt stopped at its GTK prerequisite because `python3` resolved to the mise installation, which does not provide `pyatspi`. The GTK layers and PostgreSQL release fixture were rerun with `/usr/bin/python3`, which provides PyAT-SPI, and passed.

Manual release gates remain separate: native Wayland acceptance, package upgrade/rollback, and exact-candidate soak.
