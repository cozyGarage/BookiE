# 0045: TCP checks with history live in a UI-free crate

- **Status**: Proposed (2026-11-10)
- **Date**: 2026-11-10

## Context

The GPUI plan lists "TCP/HTTP checks with history" as the first Phase 3 DBA module. [ADR 0041](0041-frontends-render-services-decide.md) says frontends render and services decide, so the check logic cannot live in a frontend.

## Decision

1. A new crate `bookie-checks` holds the logic: `check_tcp(address, limit)` returns `Outcome::Up { millis }` or `Outcome::Down { reason }`, and `History` keeps the newest N outcomes with a `Summary` (checks, up count, up percent, slowest, last).
2. It depends on `tokio` only and on no other workspace crate; `scripts/check-crate-boundaries.py` allows it no workspace dependencies.
3. HTTP checks, scheduling and persisting the history are not in this decision. They need a client dependency review and a place to store history, and follow in their own change.
4. No frontend shows it yet.

## Consequences

A frontend can poll a host and port and render the summary without new logic. The first slice is small on purpose, so the shape can change before a frontend depends on it.
