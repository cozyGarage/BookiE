# 0042: dbx is the visual reference for the GPUI experiment

- **Status**: Accepted (2026-11-11)
- **Date**: 2026-11-09

## Context

The GPUI experiment needs a target look. The maintainer chose the open-source client dbx (github.com/t8y2/dbx, Tauri 2 and Vue 3; Apache-2.0 at commit `39a77b5` on 2026-11-09, while the experiment's own notes recorded MIT for version 0.6.65) as the main inspiration. dbx publishes its design tokens in `apps/desktop/src/styles/tokens.css`: a near-monochrome neutral palette with light and dark sets, four status colours, 4 and 6 px radii, a compact density, the Geist font and a tabular numeric face for the grid. Its layout is a top toolbar, a left connection tree, tabbed editor groups, an editor above a result switcher, and a toggleable right panel. dbx has no ADRs and states no reason for its stack, so these are observations, not its decisions.

## Decision

1. The GPUI experiment's theme and layout follow dbx: the token values, radii, density and layout above, expressed in `experiments/gpui-grid-prototype/src/theme.rs` and `chrome.rs`.
2. Token values and layout ideas may be copied. The dbx notice for the licence in force when a value is copied, and the Geist SIL OFL notice, are recorded in `docs/adopted-code-attribution.md`. No dbx logo, name, icon or screenshot is used.
3. The GTK application keeps libadwaita's look ([ADR 0002](0002-rust-gtk4-libadwaita.md)). This ADR does not change the shipped UI.
4. dbx's code structure is not copied: its monolithic components and 3,000-line global stylesheet are what we avoid ([ADR 0041](0041-frontends-render-services-decide.md)).

## Rationale

Apache-2.0 and MIT both allow reuse in an AGPL-3.0 project with the notice kept. A published token set gives a concrete target instead of a taste argument.

## Alternatives considered

- Follow libadwaita in GPUI: matches GTK but gives no new look to evaluate.
- Design our own tokens: slower and untestable against a known reference.
- Copy dbx front-end code: it is Vue, so none of it runs in GPUI.

## Consequences

- If dbx changes its licence, already adopted values keep the Apache-2.0 terms they had when copied.
- The look can drift from dbx later; this ADR sets the starting point, not a standing sync.
