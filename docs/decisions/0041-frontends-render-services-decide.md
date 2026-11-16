# 0041: Frontends render; app-services decides

- **Status**: Accepted (2026-11-11)
- **Date**: 2026-11-09

## Context

[ADR 0018](0018-headless-app-services-crate.md) moved GTK-free logic into `bookie-app-services`, and [ADR 0037](0037-results-crate-split.md) split the pure result logic into `bookie-results`. The GPUI experiment ([ADR 0028](0028-isolated-frontend-experiments.md), [ADR 0039](0039-gpui-source-for-experiments.md)) still keeps its own paging, find, filter, edit-tracking and column-width logic in `experiments/gpui-grid-prototype/src/model.rs`, next to versions in app-services that the GTK application uses. Two copies drift. The upstream client this project adopted code from separates its UI-free `core` crates from its one GTK crate on purpose: replacing the GUI framework rewrites only that crate (upstream ADR notes, read 2026-11-09).

## Decision

1. Logic that decides behaviour lives in UI-free crates: `bookie-core`, `bookie-results`, `bookie-app-services`. This covers result paging, find, filter, compare, edit tracking, result editability, column widths, driver capabilities and statement guarding.
2. A frontend holds view state (focus, scroll, selection geometry, animation) and renders. It asks a service and draws the answer.
3. An experiment under `experiments/` may depend on the workspace crates `bookie-core`, `bookie-results` and `bookie-app-services` only. It may not depend on a driver, `bookie-storage`, `bookie-transport` or `bookie-policy` directly.
4. `scripts/check-crate-boundaries.py` checks rule 3 from each experiment's `Cargo.toml`.
5. Moving a duplicated rule from the experiment into a service must keep the GTK behaviour and tests unchanged. The experiment's copy is then deleted, not kept as a second implementation.

## Rationale

One implementation means a fix lands for every frontend, and a rule such as exact decimal editing is tested once. A path-dependency check is cheap and catches the common way a prototype grows its own logic.

## Alternatives considered

- Keep the experiment self-contained: simplest, but it measures a renderer against logic the product does not run.
- Let experiments use drivers directly: bypasses `PolicyGuard`, which [AGENTS.md](../../AGENTS.md) forbids for any consumer path.
- A trait-based UI abstraction over GTK and GPUI: adds a layer nobody asked for; shared services are enough.

## Consequences

- Moves into `bookie-results` need the check from ADR 0037 (no dependency beyond `core`).
- Experiments that need app-services pull its dependency graph into their lockfile and `cargo deny` surface.
- This does not allow an experiment to open user databases; ADR 0028 and ADR 0036 still apply.
