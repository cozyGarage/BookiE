# 0044: The GPUI app lives under apps/

- **Status**: Accepted (2026-11-10)
- **Date**: 2026-11-10

## Context

The maintainer chose GPUI as the next frontend ([ADR 0039](0039-gpui-source-for-experiments.md), 2026-11-09) and let it read real data through app-services ([ADR 0043](0043-gpui-frontend-may-read-real-data-through-app-services.md)). It no longer renders synthetic rows only, so `experiments/` no longer describes it. ADR 0043 item 6 keeps shipping behind separate accessibility and platform evidence.

## Decision

1. The GPUI application moves from `experiments/gpui-grid-prototype` to `apps/gpui` and the package is `bookie-gpui`. The Tauri comparison prototype stays under `experiments/`.
2. It keeps its own `Cargo.toml` with an empty `[workspace]` table and its own lockfile until a later decision joins it to the workspace; the GTK app stays the shipped product ([ADR 0002](0002-rust-gtk4-libadwaita.md)).
3. The dependency rule from [ADR 0041](0041-frontends-render-services-decide.md) and ADR 0043 still applies, and `scripts/check-crate-boundaries.py` now checks `apps/gpui/Cargo.toml` with the same allow-list: `bookie-core`, `bookie-results`, `bookie-app-services` and `bookie-driver-registry`.
4. Moving it is not shipping it. No package, release or store listing includes it until the evidence in ADR 0043 item 6 exists.

## Consequences

CI and the profile script point at the new path. ADRs 0028, 0039, 0041, 0042 and 0043 keep their original text and paths as history. The sentence in `AGENTS.md` about isolated experiments applies to `experiments/` only.
