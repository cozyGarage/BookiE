# Database workspace

Draft behavior extracted from the native GTK implementation. [AGENTS](../../AGENTS.md) and accepted ADRs remain authoritative.

## Editor

**Run** and `Ctrl+Enter` execute the selected text when a non-blank selection exists, otherwise the whole buffer. The Run menu also exposes the current statement at the cursor, equivalent to `Ctrl+Shift+Enter`. Existing admission guards refuse overlapping runs. The same statement planner, parameter binding, policy and audit pipeline handles both actions ([ADR 0016](../../docs/decisions/0016-governed-consumer-seam.md)).

When the owned editor session reports an open transaction, a separate row offers commit-and-close and rollback-and-close. Actions capture the session UUID and recheck live connection identity, transaction state, execution, lexing, opening and teardown state before dispatch. These are the existing controlled session-end operations, not autocommit switches or shared-connection transaction SQL ([ADR 0008](../../docs/decisions/0008-connection-and-session-ownership.md)). The reported commit outcome is preserved.

File output choices identify a statement and result set from the latest run. Loaded-result export captures that result snapshot and does not rerun SQL. Pinned older runs are excluded. A completed streamed result offers a separate all-row action that reads the saved page spool without re-executing SQL. There is no direct execution-to-file mode for arbitrary scripts.

## Navigation

The header toggle controls the sidebar in both layouts. A docked sidebar uses `GtkPaned` and its pointer/keyboard handle. It is constrained to 220–520 pixels while preserving at least 600 pixels for the workspace. At the existing 960sp breakpoint, the same sidebar moves into the adaptive overlay. Automatic post-connect reveal keeps a narrow overlay closed; explicit Reveal in Sidebar opens it.

Each table or view has a separate field expander. The request uses its captured guarded connection identity, a controlled timeout and cancellation. Collapse cancels the request and releases retained previews. Reply serial and connection identity are checked before display. A component removed by catalog rebuild cannot update its replacement. Four requests may run concurrently; no waiting queue is added. The UI retains at most 256 fields per object and 2,048 fields across the sidebar, with name/type previews bounded to 512 characters each. Limit, loading and failure states are explicit; collapse/expand retries. These limits bound UI retention, not the driver's initial metadata allocation.

Name and engine type are separate labels in expanded fields; type is secondary and exact case is retained. The full Structure view remains available. The existing explicit-node ListBox remains a partial implementation of [ADR 0012](../../docs/decisions/0012-sidebar-object-tree.md); this does not claim its TreeListModel migration is complete.

Grid headers use the public ColumnView title and header-menu APIs. The name precedes the exact engine type on a second line; comments move into Column details alongside key and nullability metadata. No private GTK child hierarchy is inspected.

## Export

One preferences dialog presents Scope before Format. Table page and loaded query result are explicit request contexts; table context is preserved for grid-menu exports as well as the bottom toolbar. A table page is already filtered and sorted by its browse request. Query output retains result columns, including joined and computed values.

On PostgreSQL, all matching table rows use one guarded repeatable-read snapshot with the browse filter and sort. When a qualified snapshot source is unavailable, choosing that scope displays the refusal and disables Export; current page remains available ([ADR 0014](../../docs/decisions/0014-full-table-export-snapshot.md)). Exporting a saved result reads its captured pages without rerunning SQL. Existing format-specific options, NULL markers, spreadsheet safety, column-fetch checks and cursor cleanup remain in force.

The optional maximum-row input accepts an empty value or a positive whole number. Invalid input disables Export. Loaded and paged sources stop at that cap and report omitted rows; the cap never enables an unsupported snapshot scope. One request targets one table or result. JSON Lines file and clipboard output preserve exact-source JSON as text and parsed JSON as nested values. CSV import and export accept validated custom delimiters of 1 to 8 characters without quotes or line breaks.

## Grid choices and session catalogs

Single-column foreign keys load up to 100 candidate values through controlled guarded reads and reuse the cell choice popover. Composite foreign keys are skipped. Scalar enum/set choices, foreign-key navigation and tab-separated paste use the normal staged-edit path. Unfiltered PostgreSQL, MySQL/MariaDB and SQL Server browse counts prefer engine estimates; unavailable estimates and active filters use exact counts. The pager distinguishes estimates from exact totals.

PostgreSQL and SQLite editor sessions expose their own temporary tables in the sidebar. Selecting a temporary table reads rows through its owning editor session. Switching the active session changes the visible group; session close removes it. Session-aware Structure editing and other-engine support remain outside the implemented slice.

## Validation

Automated tests cover run-scope dispatch, transaction actions, sidebar expansion, divider transitions, metadata reply ownership, field budgets, export scope and snapshot refusal. Installed desktop and accessibility behavior requires separate manual checks.

## Editor bottom panel

The existing vertical editor/result split has a persistent Results toggle, a result-set view action, an Errors view and maximize/restore controls. Ctrl+J is captured within the owning editor subtree, including its result grid. Hiding changes widget visibility only and never dispatches SQL or rebuilds the result. Native divider resizing retains a bounded desired panel height during the open editor lifetime; maximize/hide transitions preserve that height and restore SQL focus through a weak widget reference. App restart persistence is not provided.

Errors reflect the latest accepted outcomes, including streamed failure and timeout. Positioned query failures retain their numeric server position through materialized and streamed outcomes. The result and Errors views offer Go to error when the sent SQL matches the original statement. The action restores a maximized editor and checks the originating buffer revision, execution generation and physical connection identity before moving the caret. Missing, invalid or rewritten positions retain readable, copyable error text. Error controls remain above scrollable error text in the result view.

Existing terminal status and result rendering remain authoritative. Supported buffered statements deliver bounded server messages as described in [known limitations](../../docs/known-limitations.md#interface); streamed/session delivery remains separate work. A shell terminal is not part of this panel. A longer-timeout rerun has no source anchor and does not offer error navigation.

For known SQL grammars, gutter marks use the planner's source ranges too, so matching SQL inside comments is not mistaken for an executed statement. Engines without a script grammar retain their existing gutter fallback.
