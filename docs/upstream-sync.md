# Adopted-code attribution

This file records the origin of code adopted into BookiE. It does not define product behavior, architecture, or release scope; those are owned by the [ADRs](decisions/README.md), [backlog](backlog.md), and [BookiE source](../ARCHITECTURE.md).

## Adopted source

BookiE and the TablePro Linux client are both AGPL-3.0, so a reviewed file may be adopted with its history of origin recorded here instead of in source comments.

| BookiE file | Upstream file (TablePro commit `86651d743`, its `diagram` UI module) | Changes |
|---|---|---|
| `crates/app-services/src/er_diagram/layout.rs`, `scene.rs`, `zoom.rs` | `layout.rs`, `scene.rs`, `zoom.rs` | Moved to the services crate and made public; comments removed; tests kept. |
| `apps/linux/app/src/ui/diagram/card_text.rs`, `draw.rs`, `palette.rs` | `card_text.rs`, `draw.rs`, `palette.rs` | Comments removed; the palette takes a light or dark theme; GTK tests registered as isolated tests. |

The model, the bounded metadata read, the canvas, the dialog and the accessible descriptions are BookiE's own. Upstream's export and its movable-card nodes are not adopted.
