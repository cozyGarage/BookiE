# 0049: Frontends talk to app-services through typed commands and events

- **Status**: Proposed (2026-11-10)
- **Date**: 2026-11-10

## Context

Each frontend calls app-services directly and handles its own loading and staleness. GTK does this through `bookie-app-services::request_ticket` ([state management](../state-management.md)); GPUI calls `ReadSession` and `LiveSession` from its own background tasks and drops replies that arrive after a newer request. The GPUI plan lists a command/event bridge for GTK, GPUI, the CLI and MCP. [ADR 0041](0041-frontends-render-services-decide.md) says services decide and frontends render, so the bridge must not move decisions into either frontend. The TablePro and red research point to a command enum with epochs; red is GPL-3.0, so this ADR records the idea and no code is copied.

## Decision

1. app-services defines a `Command` enum for every request a frontend may make (open, list, read page, run query, sample, check, and later writes), and an `Event` enum for every answer or state change. Both are plain data with no UI types.
2. A frontend submits a command with the epoch it last saw for that tab or window. app-services answers with an event carrying the same epoch. A frontend applies an event only if its epoch is still current; this replaces the per-frontend ticket logic.
3. Events reach a frontend through a sink trait, the same pattern `TabChangeTracker` already uses (`TrackerEventSink`). A sink never blocks app-services.
4. Every command goes through `PolicyGuard` as it does now. The bridge adds no path around the guard, and a command that would write is refused unless its own ADR says otherwise.
5. Phase 1 covers reads only: open, list, read page, run query and the new subscription and status reads. Writes, the write lock and the ClickHouse commit resolver are out of scope until a frontend can show their prompts.
6. Nothing in this decision is shipped to GTK. GTK keeps `request_ticket` until a migration ADR moves it.

## Consequences

A second frontend or the CLI reuses one command set and gets staleness handling for free. The cost is one more layer that every new feature must pass through, so the command set must stay small. Until a frontend uses it, the enum is untested in production; the first use must add tests that fail if an out-of-date epoch's event is applied.
