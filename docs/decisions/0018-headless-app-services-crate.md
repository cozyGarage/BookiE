# 0018: Move GTK-free application logic into a headless crate

- **Status**: Accepted
- **Date**: 2026-10-12

## Context

Application state and database-facing logic currently sit beside GTK widgets. The UI needs a headless boundary so state transitions and service behavior can be tested without a display, while widgets remain owned by the GTK application.

[ADR 0016](0016-governed-consumer-seam.md) defines a shared service facade for GUI and agent consumers. [ADR 0003](0003-relm4-architecture.md) keeps reusable state logic outside widget construction. This decision defines that boundary.

## Decision

Create a workspace crate for application logic that has no GUI dependency. It may depend on `tablepro-core`, `tablepro-policy`, `tablepro-storage` and `tablepro-transport`; it must not depend on `gtk4`, `libadwaita`, `glib` or `relm4`.

Staged moves, each its own reviewed change:

1. The 33 GTK-free service files move as they are. First batches moved: `approval_wait`, `column_widths`, `config_io` (the glib atomic write is now a std temp-file, fsync and rename), `filter_settings`, `state_file`, `ordered_refresh`, `read_scopes`, `recent_tabs`, `request_generation`, `single_flight`, `structure_tracker`; the app re-exports them from `services` so call sites did not change. Public names and behaviour do not change.
2. The glib-touching files are split: the logic moves, a thin adapter for configuration paths and settings stays in the application.
3. A tab and workspace model is extracted from `ui/app/workspace_tabs.rs`: the list of tabs, which tab is closing, preview versus kept, and the steps "save then close" and "replace preview". It is plain data with transitions that the GTK layer calls and renders. The `AdwTabView` handshake stays in the widget layer and becomes a thin consequence of the model's state.

Constraints that come from the existing ADRs:

- The crate holds only `PolicyGuard`-wrapped connections and adds no second way to execute SQL ([ADR 0016](0016-governed-consumer-seam.md)).
- A move must not change any persisted format, path, keyring schema, application ID or connection identifier. The existing serialisation round-trip tests move with the code and must pass unchanged ([ADR 0009](0009-persistence-and-identity-compatibility.md)).
- The crate does not add a runtime plug-in boundary ([ADR 0001](0001-no-plugin-system.md)).

## Rationale

Unit tests run without a display and in parallel with the UI build. The tab model can be tested for the exact sequences that failed (close while saving, replace a preview, switch connections and back). `bookie-agentd` and the MCP service can later call the same facade as the GUI, which is the divergence ADR 0016 warns about.

## Consequences

Accepted:

- A one-off churn of imports and a larger workspace, with its own coverage and size baselines.
- A boundary that must be kept: reviewers reject a `gtk4` or `glib` import in the new crate.
- The first stage is mechanical and the third is a real refactor of the orchestration layer; it needs focused logic tests and GTK scenarios for the widget boundary.

Gained:

- Faster feedback for logic changes and a path to testing tab and workspace behaviour without a display.
- A smaller application crate and a smaller recompile when only widgets change.

## Alternatives considered

- **Leave the code where it is and add more GTK scenarios.** Widget scenarios exercise display integration, but they do not replace display-independent tests for application logic.
- **Split the UI into many GTK crates instead.** Helps compile parallelism but leaves logic and widgets coupled.
- **Move everything at once.** Rejected: it would mix a mechanical move with a behavioural refactor in one unreviewable change.
- **Put application orchestration in the domain or driver crate.** Rejected. `core` stays a domain and driver leaf; a core-to-driver dependency would reverse the driver-to-core boundary. Shared services sit above `core` and the drivers, below consumer roots, with a dependency allowlist. Orchestration lives in this crate behind the guard of [ADR 0016](0016-governed-consumer-seam.md).
