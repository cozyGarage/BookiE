# ADR implementation status

What has and has not been built for each accepted [decision](README.md). An ADR records the decision; this page records progress against it, so it may change as work lands. Each entry keeps its date. Facts about a decision's meaning stay in the ADR.

Moved from the ADRs on 2026-10-28; the text is unchanged. Treat dated entries as snapshots and recheck current source and the owning issue before relying on them.

## ADR 0011: Page large editor results through a cursor

Decision: [ADR 0011](0011-paged-query-results.md).

### Implementation status

Core and PostgreSQL cursor support and PostgreSQL read-only snapshot sessions have landed. `PolicyGuard` authorizes one read statement in a read-only snapshot, masks each fetched page, and records page and terminal audit outcomes. PostgreSQL is the only driver with cursor support; other drivers return `Unsupported`. The SQL editor uses this path for supported single-statement PostgreSQL `SELECT`s, retaining pages within the 64 MiB estimated-row budget and keeping the newest page even if it alone exceeds that estimate. PR #1660 adds a process-local encrypted spool, so pages accepted into the spool remain revisitable without rerunning the query. The spool has a shared 1 GiB quota and a 4,096-page per-result cap; when storage, quota or page limits prevent writing a page, the stream stops as incomplete and previously stored pages stay available while their owners remain open. The installed 100,000-row PostgreSQL GTK scenario traversed every page and reloaded earlier pages while keeping a transaction marker unchanged ([spool evidence](../evidence/stream-result-spool-2026-10-25.md)).

Historical Omarchy captures predate this spool: the installed package at source `d9b5aaca` retained rows 70,001–100,000 for a wide 100,000-row result ([installed profile](../evidence/native-wayland-profile-2026-10-24/README.md)), and the source-built profile at `58805ca17fcf4e450a43cde9207287741bbaa53d` retained rows 1,855,001–2,000,000 from a two-million-row result ([profile](../evidence/postgres-stream-native-wayland-2026-10-25/README.md)). A separate PR #1568 candidate capture is recorded [here](../evidence/built-head-2026-10-24/README.md#native-result-cache-capture-from-pr-commit-748c75c). These dated runs establish neither a process-memory ceiling nor general release acceptance. Other engines and unsupported query shapes keep the capped materialized path. Driver decoding also applies the result byte/cell budget per page. A historical synthetic profile reached 1,325 MiB RSS with four pinned results and one active stream ([evidence](../evidence/stream-retention-synthetic-2026-10-16/README.md)); it predates the current cache behavior and does not qualify current performance. Broader real-query memory profiling and lifecycle acceptance remain under [#1032](https://github.com/cozyGarage/BookiE/issues/1032).

## ADR 0012: Show database objects in a collapsible sidebar tree

Decision: [ADR 0012](0012-sidebar-object-tree.md).

### Implementation status

The UI-11 slice implements collapsible Tables and Views groups, per-connection collapse state, and search expansion using the existing `GtkListBox` and explicit tree-node model. It does not implement the `GtkTreeListModel` migration or lazy in-place loading of Routines, Sequences and Types. The completed UI-11 record is in the [archived pre-migration ledger](../archive/known-issues-pre-project-migration-2026-10-12.md); those partial results do not mean every part of this decision has shipped. The remaining UI-23 catalog work is tracked in [issue #552](https://github.com/cozyGarage/BookiE/issues/552).

## ADR 0017: Official Rust MCP SDK behind the governed seam

Decision: [ADR 0017](0017-mcp-sdk-behind-the-governed-seam.md).

### Implementation status (2026-10-14)

The production server has not migrated. The default stdio and streamable-HTTP entry points still use the hand-written server in `crates/mcp/src/server.rs`. `rmcp` remains optional behind `rmcp-prototype`, whose prototype serves stdio only. HTTP middleware, compatibility for every current token location, request-cancellation wiring and the full MCP regression suite against the SDK remain incomplete. The accepted direction is not yet a shipped capability.

### Prototype result (2026-10-12)

`crates/mcp/src/rmcp_server.rs`, behind the off-by-default cargo feature `rmcp-prototype`, serves the existing `McpBridge` through `rmcp` over stdio. It implements `list_tools` and `call_tool` by hand so `tools::dispatch` and the existing tool descriptions and schemas are reused unchanged. Seven tests pass over the SDK's wire:

- `tools/list` is refused without a token and with a bad token, and lists all 11 tools for a valid token.
- `tools/call` with the token in the arguments works, and without a token or with a bad token returns an error result and runs nothing.

Findings that change the plan:

- The SDK exposes the request's `_meta` (so `_meta.authorization` works) and a cancellation token per request (`RequestContext::ct`), which is what disconnect-aware cancellation needs.
- A top-level `params.token`, one of the three places clients may send a token today, is dropped by the SDK's typed parameters. A test documents the gap. Closing it needs a small transport shim that moves it into `_meta` before the SDK parses the message, or a deliberate deprecation of that location.
- The streamable-HTTP service, the `Origin` and loopback checks as middleware, and wiring `RequestContext::ct` to the approval and execute path are not in the prototype yet; they are the next stages.
- `cargo deny` passes with the new optional dependency.

## ADR 0018: Headless application-services crate

Decision: [ADR 0018](0018-headless-app-services-crate.md).

### Implementation status (2026-10-26)

Several GTK-free service modules have moved to `crates/app-services`; the crate has no GTK, libadwaita, glib or Relm4 dependencies. The headless tab model and workspace persistence live in `crates/app-services/src/workspace.rs` and `workspace_state.rs`. Completion metadata cache, request deduplication, connection-aware result application and cancellation state live in `crates/app-services/src/completion.rs`, which also performs the controlled column read. Named-parameter binding and local database-file classification/saved-record construction were added in aggregate [#1663](https://github.com/cozyGarage/BookiE/pull/1663). Driver-error classification now lives in GTK-free app-services through [#1683](https://github.com/cozyGarage/BookiE/pull/1683); the GTK app retains Relm4 task dispatch and localized error rendering. The connection dialog's auth/endpoint visibility state and typed local-socket path selection are in `crates/app-services/src/connection_form.rs` on [#1686](https://github.com/cozyGarage/BookiE/pull/1686), with widget updates and path display remaining in GTK. SQL editor text budgets, UTF-8 boundary checks and dropped-text validation are being moved to `crates/app-services/src/sql_text.rs` under [#1299](https://github.com/cozyGarage/BookiE/issues/1299); the GTK adapter retains buffer-position extraction and localized refusal messages. File-open signals/dialogs/toasts, driver lookup, and routing results to the shared index also remain in the GTK app. GTK page handling remains in `apps/linux/app/src/ui/app/workspace_tabs.rs`. The staged migration remains active under APP-MOD in the [post-0.2.0 backlog](../backlog.md).

## ADR 0022: Release notes come from merge commits

Decision: [ADR 0022](0022-release-notes-from-merge-commits.md).

### Implementation status

Landed: the trailer grammar, `check`, `trailers` and `assemble` in `scripts/release-notes.py` with tests, the PR template line, the documentation, and `.github/workflows/release-note-check.yml`, which validates pull-request bodies. Whether GitHub requires that check in the branch ruleset is not recorded here. `assemble` was run on the real history after the guard (22 notes, every pull request merge carries one); it skips direct pushes and merge commits without a note, and a note wrapped across lines keeps its whole sentence. Not landed: the `release.yml` step that calls `assemble` instead of reading `CHANGELOG.md`. Until then, the first cut must reconcile as described in point 4.

## ADR 0015: Database snapshots and restore

Decision: [ADR 0015](0015-database-snapshots.md).

### Clarification (2026-10-20)

This ADR records a design, not a release commitment. Snapshot implementation is post-0.2.0 backlog work tracked by [#547](https://github.com/cozyGarage/BookiE/issues/547); no release target is set. The context predates PostgreSQL logical backup and restore in ADR 0019. Those portable dump tools do not implement this snapshot design or provide the same database-state semantics.

## ADR 0019: Database tooling without sidecars

Decision: [ADR 0019](0019-database-tooling-without-sidecars.md).

### Clarification (2026-10-20)

The accepted decision does not set a release target for the remaining tools. PostgreSQL backup and restore are implemented; follow-up work remains subject to post-0.2.0 selection and acceptance.

The implementation uses the shared transport and SSH route to create the client-tool endpoint, then supervises the vendor process in `crates/dump`; it is not specific to the system OpenSSH backend. The current process does not apply Landlock, so the conditional filesystem restriction in Decision 3 is not implemented and must not be treated as a shipped security boundary.

## ADR 0021: Support only the newest GTK toolkit

Decision: [ADR 0021](0021-toolkit-floor.md).

### Implementation clarification, October 23

The accepted decision initially held GTK at `gnome_47` because the shortcuts window still used `GtkShortcutsWindow`. Replacing it with `AdwShortcutsDialog` allowed the GTK Rust features to advance to `gnome_50` plus `v4_24`, and GLib to `v2_88`, matching the runtime floor. Toolkit requirements and distribution support did not change.
