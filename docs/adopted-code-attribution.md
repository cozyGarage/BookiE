# Adopted-code attribution

This file records source attribution for code adopted into BookiE. It does not define product behavior, architecture, or release scope; those are owned by the [ADRs](decisions/README.md), [Project #3](https://github.com/users/cozyGarage/projects/3), and [BookiE source](../ARCHITECTURE.md).

## Adopted source

BookiE is an independent project. It does not track, merge from or sync with TablePro; this file only records the origin of code that was adopted once.

BookiE and the TablePro Linux client are both AGPL-3.0, so a reviewed file may be adopted with its history of origin recorded here instead of in source comments.

| BookiE file | Source file (TablePro commit `86651d743`, its `diagram` UI module) | Changes |
|---|---|---|
| `crates/app-services/src/er_diagram/layout.rs`, `scene.rs`, `zoom.rs` | `layout.rs`, `scene.rs`, `zoom.rs` | Moved to the services crate and made public; comments removed; tests kept. |
| `apps/linux/app/src/ui/diagram/card_text.rs`, `draw.rs`, `palette.rs` | `card_text.rs`, `draw.rs`, `palette.rs` | Comments removed; the palette takes a light or dark theme; GTK tests registered as isolated tests. |

The model, the bounded metadata read, the canvas, the dialog and the accessible descriptions are BookiE's own. The export and movable-card nodes are not adopted.

## Adoption procedure

1. Read the candidate file and diff it against the BookiE counterpart before copying. Adopt only what BookiE lacks.
2. Port it to BookiE's types, remove comments and add a test that fails without it.
3. Add a row to the table above with the source commit and the changes.
