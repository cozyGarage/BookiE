# 0018: Move GTK-free application logic into a headless crate

- **Status**: Accepted
- **Date**: 2026-10-12

## Context

Application state and database-facing logic sit beside GTK widgets. The UI needs a headless boundary so state transitions and service behavior can be tested without a display, while widgets remain owned by the GTK application.

[ADR 0016](0016-governed-consumer-seam.md) defines a shared service facade for GUI and agent consumers. [ADR 0003](0003-relm4-architecture.md) keeps reusable state logic outside widget construction. This decision defines that boundary.

## Decision

The workspace crate for application logic has no GUI dependency. It may depend on `tablepro-core`, `tablepro-policy`, `tablepro-storage` and `tablepro-transport`; it must not depend on `gtk4`, `libadwaita`, `glib` or `relm4`.

The crate holds only `PolicyGuard`-wrapped connections and adds no second way to execute SQL. Persisted formats, paths, keyring schema, application ID and connection identifiers remain owned by their existing contracts. GTK event handling and widget lifecycle remain in the application.

## Rationale

A headless crate allows application state and service behavior to be tested without a display. It also lets the GUI and `bookie-agentd` use the same services without moving application orchestration into the domain or driver layers.

## Consequences

- GTK-free behavior can be tested independently from widget integration.
- GTK dependencies stay in the application composition and presentation layers.
- The `PolicyGuard` boundary and persistence contracts remain unchanged.

## Alternatives considered

- **Leave the code in widget modules and add more GTK scenarios.** Widget scenarios exercise display integration but do not replace display-independent tests for application logic.
- **Split the UI into many GTK crates.** This helps compile parallelism but leaves logic and widgets coupled.
- **Move application orchestration into the domain or driver crate.** `core` remains a domain and driver leaf; shared services sit above `core` and drivers, below consumer roots.
