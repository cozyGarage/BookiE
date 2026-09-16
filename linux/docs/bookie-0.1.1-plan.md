# BookiE 0.1.1 — next sprint and release plan

Superseded for sequencing by Milestone A of the approved
[BookiE 0.1.1 → 0.2 sprint](bookie-0.2-sprint.md). Evidence below is historical.

Status: proposed implementation plan for user review; no rename, version bump,
new feature, release tag or publication has been performed by this document.
Prepared: 2026-09-14. Delivery branch: `linux`; Linux remains the priority.

## Decisions and scope

- Product name: **BookiE**, exactly this capitalization. The personal inspiration
  is the bookmaking world in Peaky Blinders; the product remains a database tool,
  not gambling software. Use original branding, not television artwork or logos.
- Target version: **0.1.1**. Do not invent a prior stable 0.1.0 release: verify the
  actual published tag/package history before choosing the upgrade baseline.
- Focus: stabilization, reproducible bug fixes and a safe rename. Security and
  data correctness take precedence over feature breadth.
- Exactly one proposed macOS feature: **Jump to Column** for wide result grids.
  This selection is for review, not permission to begin implementing today.
- No new engines, broad redesign, full database transfers, backup/restore,
  materialized-view mutations or Apple-source merge in this sprint.

## Starting point: what is done and what is not

Implementation baseline: `30b7e530f`, pushed to `fork/linux` in
`cozyGarage/TablePro`. See the [September 14 evidence ledger](sprint-2026-09-14.md).
It includes dialect-aware IN literals and SQL lexing, Redis browse isolation,
shared exact-value exports, cancellable atomic loaded-row file export,
connection-owned catalog refresh, MongoDB numeric conversion and SQL warnings.

Passed locally on that code: full fast CI/strict Clippy, 47 driver integration
checks, PostgreSQL release fixtures, 23 driver TLS checks, 18 installed GTK
scenarios and fresh dependency checks. The accepted `paste` maintenance advisory
remains. These are baseline results, not evidence for future 0.1.1 changes.

Still unverified for release: exact-candidate hosted CI/MSRV, optional DuckDB,
Arch install/upgrade/rollback, and the established GTK soak requirement. Redis
and MongoDB remain experimental. MacOS review is source/behavior comparison, not
a claim of having run or tested the macOS application.

Pre-existing manual-connection files, the README edit and old RC package files
were deliberately excluded from the sprint commit. Review them independently;
do not stage them wholesale or treat old artifacts as BookiE 0.1.1 candidates.

## Ordered work and acceptance

### 1. Establish the release baseline and fix defects first

- [ ] Inspect worktree and remote history; preserve unrelated edits. Record the
  exact starting SHA, supported Rust toolchain and published upgrade source.
- [ ] Review hosted results for the pushed sprint; reproduce failures locally
  where possible. Do not dismiss a failure because an earlier local run passed.
- [ ] Audit changed SQL, export, catalog, Redis and MongoDB paths. Track each
  finding with severity, reproduction, affected code, failing regression, fix,
  verification and disposition. Distinguish bugs from unsupported capabilities.
- [ ] Cover cancellation/timeout and reconnect races, stale metadata completion,
  failed/rolled-back DDL, editor teardown during diagnostics, duplicate/hidden
  columns, exact numeric/binary values, and export errors/cancel-before-publish.
- [ ] Fix confirmed defects in small reviewable commits. No open critical/high
  security or data-loss defect may be waived merely to meet the version target.

### 2. Adopt one feature: Jump to Column

Why this feature: useful to DBA/data-engineering work, scoped to existing grid
metadata, and adds no database writes or new driver surface. The Linux grid has
no integrated column-jump search found in this review.

Source reference inspected: cached upstream main
`465f306e41cadbe1a2a69441255eb4626f14aa75`, particularly
`MainContentCoordinator+ColumnJump.swift`, `DataGridView+ColumnJump.swift` and
`JumpToColumnMenuValidationTests.swift`. The prior review separately verified
remote main through `035ffe8f771797345ad86032e3aead16f18181fe`. Recheck the exact
feature reference at implementation time; do not describe the cached ref as
current remote main.

- [ ] Implement a native GTK searchable column picker for both browse and SQL
  result grids; use existing loaded column metadata, not an extra SQL query.
- [ ] Bind its entries to the originating window/tab/result generation and
  stable source ordinal. Display duplicate names unambiguously; never identify
  a column solely by its label or stale visual position.
- [ ] On activation, reveal a hidden column if necessary, scroll horizontally
  to it and focus the relevant grid/header without changing data or sort/filter
  state. Verify GTK feasibility first; document any proposed UX deviation.
- [ ] Enable the action only for a connected, eligible grid with columns.
  Cancel safely on tab/result replacement, disconnect or window closure.
- [ ] Support keyboard search, arrows, Enter and Escape; select a conflict-free
  shortcut and document it in the shortcuts UI. Preserve accessible names.
- [ ] Unit-test filtering, Unicode/duplicate names and ordinal resolution;
  installed GTK tests must cover wide grids, hide/reorder, no match, empty
  results, stale picker activation and two-window ownership.

Completion means one integrated workflow with tests, not a second feature such
as row search, new filter syntax or browse history. If it threatens reliability,
request a scope decision rather than quietly adding or substituting features.

### 3. Rename TablePro Linux to BookiE safely

Perform after functional fixes so branding changes do not obscure bug review.
Do not use a global search-and-replace across source, history and persistence.

| Surface | Proposed 0.1.1 treatment |
| --- | --- |
| Window/About text, menus, screenshots, active docs | BookiE; original icon/wordmark; update translations and attribution |
| Package and primary commands | Propose `bookie` and `bookie-agentd`; validate availability and transition metadata before finalizing |
| Old commands/package | Compatibility wrappers or aliases; test Arch `provides/conflicts/replaces` decisions with actual install and rollback |
| Application ID, D-Bus, desktop/Flatpak IDs | Retain `com.tablepro.linux` for 0.1.1 unless a separately reviewed migration is required; visible name can differ |
| Saved-state directories and keyring schema | Retain legacy `tablepro` paths and `com.tablepro.linux.Password` initially; no credential loss or split histories |
| Cargo crates, internal modules, environment variables | Keep compatible internal names initially; avoid a cosmetic workspace-wide churn |
| MCP tools/protocol, token scope, audit schema | Preserve contracts, connection UUIDs and hash chains; update display text only where compatible |
| Repository URL, release tag prefix, Flatpak publishing identity | Explicit owner decision before external rename/publication; do not change remotes automatically |
| Historical evidence and upstream attribution | Preserve original names and recorded versions; label historical documents instead of rewriting history |

The product name is decided; lowercase executable/package names and durable
identity choices above are recommendations for review. Check naming availability
and applicable branding concerns before publication; this plan claims no name
clearance or repository/domain ownership.

- [ ] Inventory actual paths/IDs across storage, preferences, favorites/history,
  keyring, SSH known_hosts, audit logs, IPC/socket locks, MCP setup, translations,
  icons, desktop/AppStream, Flatpak, Arch, CI and scripts.
- [ ] Prefer compatibility over migration for 0.1.1. If any durable identifier
  must change, specify an idempotent, bounded, atomic migration with backups,
  conflict handling and recovery first; never silently delete legacy data.
- [ ] Test old-install upgrade and rollback with saved connections, credentials,
  TLS/SSH configuration, favorites, queries, workspace, tokens and audit history.
  Verify one intended launcher/instance, old command compatibility and working
  agent configuration. Never probe the user's live keyring with test secrets.
- [ ] Resolve versions across Cargo/lockfile, package metadata, About, AppStream,
  changelog and release tooling. Do not assume existing RC-only scripts can
  build or publish 0.1.1 unchanged.

### 4. Freeze and verify the exact release candidate

- [ ] Clean stale active docs/dead code only with evidence; preserve historical
  ledgers and upstream attribution. Update driver maturity claims honestly.
- [ ] Freeze one candidate SHA and rerun full CI, Rust 1.93 MSRV/current stable,
  driver integrations, PostgreSQL release, TLS, isolated Secret Service,
  installed GTK, optional DuckDB and dependency audit/deny gates as applicable.
- [ ] Record hosted results for that SHA. A failed, skipped or unavailable gate
  remains explicitly pending; a later code change invalidates affected evidence.
- [ ] Build traceable Arch artifacts; record SHA/version/checksum and run package
  content, `namcap`, desktop/AppStream and supported packaging checks.
- [ ] Give the user the exact artifact and install command for Arch/Hyprland;
  obtain fresh install, upgrade and rollback evidence on the intended package.
- [ ] Meet the existing soak gate: 30 consecutive retry-free GTK attempts over
  six runs, plus installed Wayland/manual behavior. Investigate flakes instead
  of counting reruns as uninterrupted success.
- [ ] Review known issues and rollback instructions. Tag/publish only after
  explicit release approval; do not overwrite old tags or publish old RC files.

## Review checklist before implementation

1. Approve Jump to Column as the single macOS feature.
2. Approve the compatibility-first rename boundary, especially retained IDs and
   storage names versus the proposed `bookie` package/commands.
3. Confirm release tag/repository naming and the real upgrade baseline before
   packaging; defer external renames until ownership and migration are decided.

Next session starts with this review and baseline checks. Today ends with code
and planning documentation committed/pushed; implementation of 0.1.1 waits.
