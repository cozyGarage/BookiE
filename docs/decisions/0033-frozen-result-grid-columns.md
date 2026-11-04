# 0033: Frozen leading columns in the result grid

- **Status**: Accepted (2026-11-04)
- **Date**: 2026-11-04

## Context

Issue #2208 asks to freeze one or more leading data columns beside the row numbers while the rest of the grid scrolls horizontally. The result grid is one `GtkColumnView` over one `GtkMultiSelection` (`ui/grid/mod.rs`). GTK4 has no frozen-column support, and `ColumnView` gives no way to keep some columns out of the horizontal scroll.

Nineteen non-test files refer to `ColumnView`: grid display, columns, column order, visibility and jump, row find, font zoom, the context menus (several find their grid through `ancestor(ColumnView)`), the streamed and paged result views and the browse tab. Any design that adds a second view has to keep all of them correct.

## Decision

Option C below: BookiE does not freeze result-grid columns for now. The maintainer chose it on 2026-11-04, to be revisited if wide tables become a real problem. Options A and B were not rejected on merit; they cost more than the benefit justifies today.

## Options

- **A. Two `ColumnView`s side by side.** A narrow frozen view and the scrolling view share the same `GtkMultiSelection` and one vertical adjustment. Reuses the existing cell, edit and width code. Costs: every `ancestor(ColumnView)` and "the grid" lookup must find the right half, horizontal scroll must not move the frozen half, and row heights and header heights must stay aligned.
- **B. One custom widget on `GtkListView`.** Full control of layout, no sync problem. Costs: a rewrite of the grid's cells, editing, find, filter and menus. Not a slice.
- **C. Do not freeze.** Keep the row-number column, "jump to column" and column visibility, and close #2208 as declined. No risk, but the wide-table problem stays.

## If this is revisited

Spike A on a throwaway branch with one frozen first column. It passes only if: selection and keyboard navigation stay in step across both halves; the find bar, local filter and sort still work; the context menus and cell editing find the right cell; column reorder and persisted widths still apply; and the GTK path stays async on a large loaded page. If any of these needs more than a small adapter, stay with C.

## Consequences

- No code changes. The row-number column, "jump to column" and column visibility remain the tools for wide tables.
- #2208 is closed as declined with a link here. A future need reopens this decision; it does not need a new record unless the chosen option changes.
