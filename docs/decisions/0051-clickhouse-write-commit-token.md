# 0051: Writes carry a commit token so an unknown outcome can be resolved

- **Status**: Proposed (2026-11-10)
- **Date**: 2026-11-10

## Context

ADR 0022-era write paths report an error when the server's answer is lost. For PostgreSQL the transaction id (`CommitToken::PostgresXid`) lets `resolve_commit` ask the server what became of a lost COMMIT. ClickHouse has no transactions. The driver now tags a write with a query id (`CommitToken::ClickhouseQuery`), and `resolve_commit` reads `system.query_log` (ADR-less driver change in #2746). No production write produces that token yet, so an unknown ClickHouse write still reports only that the outcome is unknown.

## Decision

1. `Connection` gets a token-returning write, `execute_controlled_with_token(sql, control) -> Result<(ExecResult, CommitToken), (DriverError, CommitToken)>`, with a default that returns `Unsupported` so every driver opts in. Drivers that already resolve commits (PostgreSQL, ClickHouse) implement it.
2. The policy guard implements it by running the same classification, approval, intent and outcome audit as `execute_controlled`. A write that is refused never runs and returns no token. The token is recorded in the outcome audit record so an operator can later resolve it.
3. agentd's connection wrapper forwards the method (the existing forwarding test enforces this).
4. Read paths are unchanged. No frontend uses the token yet. The GTK write prompt for an unknown outcome is a separate change.

## Consequences

An unknown ClickHouse write can be resolved from the audit record and the server's query log, instead of being reported only as unknown. The cost is one more method on the trait and one more audited path to keep in step with `execute_controlled`.
