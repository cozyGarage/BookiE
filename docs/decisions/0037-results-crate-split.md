# 0037: Split the pure result logic into `bookie-results`

- **Status**: Proposed
- **Date**: 2026-11-04

## Context

`bookie-app-services` holds both application state (connections, workspace, preferences) and pure result logic: compare, filter, find, predicate, scan, spool streaming, exact values and the sidebar tree. The pure part needs only `bookie-core`, yet any frontend that wants it (the GPUI and Tauri experiments, a future non-GTK client) must build all of app-services with `dump`, `policy`, `storage` and `transport`. ADR 0018 already moved GTK-free logic out of the app; this takes the next step for the part with the fewest dependencies. Issue #2443.

## Decision

Move `result_compare`, `result_filter`, `result_find`, `result_predicate`, `result_scan`, `sidebar_tree`, `stream_result_spool` and `value_exact` into a new crate `bookie-results` that depends on `bookie-core` and no other workspace crate. `bookie-app-services` re-exports each module under its old name, so existing `bookie_app_services::result_find` paths keep working. The test that cross-checks `value_exact` against app-services' `KeyValue` stays in app-services.

## Consequences

- Behaviour is unchanged; the moved tests run in the new crate and the app-services suite still passes.
- The import DAG gains `bookie-app-services ──► results ──► core`; `scripts/check-crate-boundaries.py` enforces that `results` imports only `core`.
- Frontends can depend on `bookie-results` alone.
- Later moves into `bookie-results` need their own check that the module has no dependency beyond `core`.
