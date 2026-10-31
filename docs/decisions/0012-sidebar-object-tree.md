# 0012: Show database objects in a collapsible sidebar tree

- **Status**: Accepted
- **Date**: 2026-10-07

## Context

The sidebar is one `GtkListBox` fed by a Relm4 factory. Schema and Views headers are list headers, and each header belongs to the first row below it. Hiding rows with the list filter therefore hides the header and its controls, so groups cannot be collapsed. Routines, sequences, types and the other catalog objects are reachable only through the Catalog dialog, and the dialog is a flat list per kind.

## Decision

Replace the list with a `GtkListView` over a `GtkTreeListModel`.

1. A node is a connection root, a schema, a group (Tables, Views, Routines, Sequences, Types), or an object. Groups are real rows, so they collapse without affecting their children's controls.
2. Only Tables and Views load with the connection, as today. A group that needs a catalog query loads on first expand, through the guarded connection, with the catalog limit and the cancellation token the Catalog dialog already uses.
3. The search entry filters objects across all groups and expands the groups that contain a match. Clearing it restores the saved collapse state.
4. Collapse state is saved per connection under the existing workspace state.
5. Opening a table, view or catalog object keeps the current messages (`SelectTable`, `EditStructureTab`, and so on). Selection sync maps a tab to its node by schema, kind and name.
6. Row context menus keep their actions. Group rows offer New Table and Table from CSV, which today sit on the schema header.

## Alternatives considered

- **Keep the `GtkListBox` and hide rows with the list filter.** Rejected as the target. A header belongs to the first row below it, so filtering hides the header and its controls and groups cannot collapse. The current `GtkListBox` implementation does not provide collapsible groups.
- **Leave catalog objects only in the Catalog dialog.** Rejected. A flat list per kind makes Routines, Sequences and Types unreachable in context.
- **A generic docking or panel framework.** Not adopted. Add one only when concrete behaviours need it. The existing editor split supports the identified results and error panel work; a generic framework is not required for it.

## Consequences

- Replacing the list requires a tree model, filtering and selection synchronization that preserve existing navigation actions.
- Accessibility roles and names must continue to describe rows and expanders.
- Catalog objects become browsable in place, which makes additional catalog objects available in context.
