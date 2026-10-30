# UI layout changes

Proposed; nothing here is accepted or scheduled. This page records candidate UI changes for [#1862](https://github.com/cozyGarage/BookiE/issues/1862), [#1863](https://github.com/cozyGarage/BookiE/issues/1863), [#1864](https://github.com/cozyGarage/BookiE/issues/1864) and [#1867](https://github.com/cozyGarage/BookiE/issues/1867). The maintainer picks the order and what to drop.

## Scope

Each candidate describes an observable UI behavior. Existing safety, accessibility and driver capability contracts continue to apply.

## Slices

Each slice is independent and keeps shortcuts and AT-SPI names unless the slice says otherwise. UX owns the GTK files; B3 owns metadata and driver capability changes.

1. **Result strip ([#1862](https://github.com/cozyGarage/BookiE/issues/1862)).** Replace the `Results` toggle, the bare list icon and `Errors (0)` with one grouped switcher with an active state. Show the error count only when it is not zero. Keep the maximize button on the right. Small, UX.
2. **Filter row ([#1864](https://github.com/cozyGarage/BookiE/issues/1864)).** One always-visible row: column, operator, value, add rule, a raw-WHERE toggle, and Apply next to the field. Move saved filters (Apply saved, Delete, Save as, Rename) into one menu. The structured rule model and the core filter builder stay as they are. Medium, UX.
3. **Editor toolbar ([#1864](https://github.com/cozyGarage/BookiE/issues/1864)).** Keep Run, Session and Explain visible; move Fold block, Unfold all, Pin results and Split into an overflow menu with their shortcuts shown. Small, UX.
4. **Large results ([#1863](https://github.com/cozyGarage/BookiE/issues/1863), [#1867](https://github.com/cozyGarage/BookiE/issues/1867)).** Keep the rows already received on screen while later pages stream; show the total as it is learned; at the end or after a timeout keep the last good page and its row range and show the timeout as a notice. Whether to load a first page and count on demand is a decision for ADR 0011 and 0025, not for this slice. Medium, UX.
5. **Sidebar tree.** Add Collapse all and Expand all buttons to the sidebar header. Add Materialized Views, Routines, Sequences and Types groups, and a Users & Privileges node, for engines whose `capabilities()` profile says the catalog kind exists. Reuse the catalog queries behind the Catalog dialog; do not add new queries per group until measured against [#580](https://github.com/cozyGarage/BookiE/issues/580). Medium, UX with B3.
6. **ER diagram.** Keep the diagram read-only. Improvements need foreign-key metadata per engine (`supports_foreign_key_metadata`), a bound on tables read at once, and keyboard and screen-reader access. Large, UX with B3.

7. **Row density.** Measure sidebar and grid row heights at narrow and wide window sizes. Consider a compact density, a wider sidebar so names remain readable, and table sizes next to names where the catalog already provides them. Small to medium, UX. Slices 1 and 4 free more height; this slice makes it usable.

## Constraints

- Values stay typed under [ADR 0007](../decisions/0007-type-and-value-preservation.md); no layout change may render a refused value as blank.
- Every statement still goes through PolicyGuard; filters and the diagram read through the guarded read path.
- Installed GTK scenarios find controls by accessible name; renaming one needs the scenario updated in the same change.
- Acceptance for each slice: a widget test, a screenshot at about 920 px and at 1,400 px or wider in light and dark, and keyboard-only operation.

## Open decisions

- Order. A reasonable first group is 1, 3 and the end-of-stream part of 4, because they are small and visible.
- Whether saved filters stay directly visible or move to a menu.
- Which sidebar groups to show by default, and whether the Users & Privileges node needs its own approval path because it exposes grants.
- Whether the ER diagram ships read-only or with later editing, and its table limit.
- Whether large results should load an initial page and total count on demand.

## Not proposed

AI actions, a JSON sidebar, account-gated features, a custom theme, or any change to what is executed or audited.
