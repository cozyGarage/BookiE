# UI layout changes drawn from peer clients

Proposed; nothing here is accepted or scheduled. It turns the [October 28 side-by-side capture](../archive/tablepro-dbx-review-2026-10-20/g-side-by-side-capture-2026-10-28.md) and the maintainer's preferences into small slices for [#1862](https://github.com/cozyGarage/BookiE/issues/1862), [#1863](https://github.com/cozyGarage/BookiE/issues/1863), [#1864](https://github.com/cozyGarage/BookiE/issues/1864) and [#1867](https://github.com/cozyGarage/BookiE/issues/1867). The maintainer picks the order and what to drop.

## What was checked first

| Claim | Evidence |
|---|---|
| Upstream TablePro Linux has an ER diagram | the `ROADMAP.md` row "ER diagram" and the GTK module `src/ui/diagram/` of the upstream Linux app crate (canvas, draw, export, keyboard description) at `origin/main` `86651d743`. Tables as cards, foreign keys in crow's foot notation, pan, zoom, drag, Reset Layout, PNG and SVG export |
| BookiE has no ER diagram | No diagram module in `apps/` or `crates/`; the [feature comparison](../0.2-feature-comparison.md) lists it as "No diagram view identified" |
| dbx can collapse the whole tree | `AppSidebar.vue` has a `collapseAllTreeNodes` action behind a "Collapse all" tooltip |
| Beekeeper Studio can expand and collapse all | `entity-list` emits `bks-expand-all` and `bks-collapse-all` from two buttons |
| dbx shows users and privileges in the tree | Observed: a "Users & Privileges" node under a PostgreSQL connection, plus per-schema Procedures, Functions, Sequences and Types. Not opened, so its contents are not described here |
| BookiE already has roles and privileges, but not in the sidebar | `catalog_dialog.rs` lists `Role` and `Grant` ("Privileges") kinds; the sidebar shows only Tables and Views, with no collapse-all or expand-all command |
| BookiE's sidebar-header list icon is the schema filter | `schema_filter_button` in `ui/app/mod.rs` |

## Slices

Each slice is independent and keeps shortcuts and AT-SPI names unless the slice says otherwise. UX owns the GTK files; B3 owns metadata and driver capability changes.

1. **Result strip ([#1862](https://github.com/cozyGarage/BookiE/issues/1862)).** Replace the `Results` toggle, the bare list icon and `Errors (0)` with one grouped switcher with an active state. Show the error count only when it is not zero. Keep the maximize button on the right. Small, UX.
2. **Filter row ([#1864](https://github.com/cozyGarage/BookiE/issues/1864)).** One always-visible row: column, operator, value, add rule, a raw-WHERE toggle, and Apply next to the field. Move saved filters (Apply saved, Delete, Save as, Rename) into one menu. The structured rule model and the core filter builder stay as they are. Medium, UX.
3. **Editor toolbar ([#1864](https://github.com/cozyGarage/BookiE/issues/1864)).** Keep Run, Session and Explain visible; move Fold block, Unfold all, Pin results and Split into an overflow menu with their shortcuts shown. Small, UX.
4. **Large results ([#1863](https://github.com/cozyGarage/BookiE/issues/1863), [#1867](https://github.com/cozyGarage/BookiE/issues/1867)).** Keep the rows already received on screen while later pages stream; show the total as it is learned; at the end or after a timeout keep the last good page and its row range and show the timeout as a notice. Whether to load a first page and count on demand (dbx, Beekeeper) is a decision for ADR 0011 and 0025, not for this slice. Medium, UX.
5. **Sidebar tree.** Add Collapse all and Expand all buttons to the sidebar header. Add Materialized Views, Routines, Sequences and Types groups, and a Users & Privileges node, for engines whose `capabilities()` profile says the catalog kind exists. Reuse the catalog queries behind the Catalog dialog; do not add new queries per group until measured against [#580](https://github.com/cozyGarage/BookiE/issues/580). Medium, UX with B3.
6. **ER diagram.** Adapt the upstream TablePro Linux diagram read-only first, with provenance and attribution, as the [earlier review](../archive/tablepro-dbx-review-2026-10-20/d-dbx-features.md) already proposed. It needs foreign-key metadata per engine (`supports_foreign_key_metadata`), a bound on tables read at once, and the keyboard and screen-reader access the upstream roadmap describes. Large, UX with B3.

## Constraints

- Values stay typed under [ADR 0007](../decisions/0007-type-and-value-preservation.md); no layout change may render a refused value as blank.
- Every statement still goes through PolicyGuard; filters and the diagram read through the guarded read path.
- Installed GTK scenarios find controls by accessible name; renaming one needs the scenario updated in the same change.
- Acceptance for each slice: a widget test, a screenshot at about 920 px and at 1,400 px or wider in light and dark, and keyboard-only operation.

## Open decisions

- Order. A reasonable first group is 1, 3 and the end-of-stream part of 4, because they are small and visible.
- Whether saved filters stay (BookiE added them to the upstream panel) or move to a menu only.
- Which sidebar groups to show by default, and whether the Users & Privileges node needs its own approval path because it exposes grants.
- Whether the ER diagram ships read-only or with later editing, and its table limit.
- Whether to adopt the peer clients' first-page loading for large results.

## Not proposed

Copying dbx's AI actions (Fix with AI, Ask AI), a JSON sidebar or other paid-tier features of the peer clients, a custom theme, or any change to what is executed or audited.
