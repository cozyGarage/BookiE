# 0046: PostgreSQL sessions and locks live in a UI-free crate

- **Status**: Accepted (2026-11-10)
- **Date**: 2026-11-10

## Context

The GPUI plan lists a sessions and lock panel as a Phase 3 DBA module. [ADR 0041](0041-frontends-render-services-decide.md) keeps decisions out of frontends, and [ADR 0045](0045-network-checks-crate.md) set the pattern: one small UI-free crate per module.

## Decision

1. `bookie-pg-sessions` reads `pg_stat_activity` and `pg_blocking_pids` through a `bookie_core::Connection` and returns `Session` values: pid, user, database, state, wait, seconds running, blocking pids, query (cut at 2000 characters). `blockers` ranks the pids that block other sessions.
2. It depends on `bookie-core` only. Its container test uses the PostgreSQL driver as a dev dependency.
3. Reading is all it does. Cancelling or terminating a backend (`pg_cancel_backend`, `pg_terminate_backend`) is a write that must go through PolicyGuard approval and is a separate decision.
4. No frontend shows it yet. A row that does not match the expected shape is skipped, not guessed.

## Consequences

A frontend can list who is connected, who waits, and who blocks them, with no new logic. Other engines need their own reader.

`bookie-app-services` re-exports `bookie-pg-sessions` and `bookie-checks` (`bookie_app_services::pg_sessions`, `::checks`), so the GPUI app reaches them through the dependencies ADR 0044 already allows. The GPUI command "Show PostgreSQL sessions and blockers" puts `SESSIONS_SQL` in the Query editor and runs it; it is listed only on a PostgreSQL connection.
