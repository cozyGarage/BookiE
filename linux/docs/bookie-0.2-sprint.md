# BookiE 0.1.x → 0.2: upstream convergence and daily workflows

Approved: 2026-09-16. Status: implementation started, no release approved.
Delivery branch: `linux`, `origin/linux` in `cozyGarage/TablePro` (called `fork`
in older documents). This document supersedes the 0.1.1 plan for sequencing.

## Goal and baseline

Continue from the integrated BookiE 0.1.4 baseline toward 0.2.0 with the full upstream Linux foundation,
daily editor/grid workflows, and read-only PostgreSQL catalog/type browsing.
Keep the original two-week checkpoint; extend the sprint until release gates pass.
The original Milestone A numbering below is historical scope, not a request to
downgrade or release 0.1.1. See the [September 17 baseline review](baseline-review-2026-09-17.md)
for the 30-commit review, remaining risks, and revised handoff order.

| Source | Pin | Review |
| --- | --- | --- |
| Our Linux | `143a389ae2b9e5abc13306d7c6f22299a94a6fb7` | Current implementation and plans |
| Upstream main | `fc8b887af7cf2b2e72c5d01f5b5b5c952c93f4e3` | 74 commits after `035ffe8f`, targeted source inspection |
| Upstream Linux | `5730238f5c72b4924669efbb7c0e79372de50a36` (dangling) | All 60 subjects after `807d28094`, targeted source inspection |

Upstream force-pushed its `linux` branch after this table was written, so the
Upstream Linux pin no longer resolves and `807d28094` is no longer an ancestor of
`origin/linux`. That pin's commit is now `f3cbf7361 fix(linux): rebuild the
GSettings schema when it changes and cover SQLite in dev-env`. The
[2026-09-19 re-survey](upstream-sync.md) re-establishes the boundary and lists
what is genuinely new.

[Build Linux](https://github.com/cozyGarage/TablePro/actions/runs/34802088529)
and [Flatpak Linux](https://github.com/cozyGarage/TablePro/actions/runs/34802088530)
passed at the baseline, including driver/TLS/PostgreSQL/GTK smoke/optional DuckDB.
No published GitHub releases were listed. This is historical evidence, not
verification of future changes. Package install/upgrade/rollback and soak remain.

### Findings

Source findings; the planning review did not rerun runtime reproductions:

- P1: PostgreSQL converts failed decoding, including wide NUMERIC and unsupported
  types, into NULL. In 0.1.1 distinguish NULL from failure; in 0.2 use lossless values.
- P1: generic SQL formatting forces uppercase without checking executable tokens.
  Adopt upstream dialect-aware token preservation; keep unsafe statements unchanged.
- P1: workspace SQL truncates at 256 KiB. Use complete off-thread per-tab drafts.
- P1: SSH secret-save failures are ignored. Surface failure and preserve recovery.
- P2: settings snapshots may finish out of order; unreadable files become empty
  state. Use ordered coalesced persistence with explicit read errors.
- P2: roadmaps incorrectly describe existing formatting/run-at-cursor and other
  completed work. Reconcile claims against code and evidence.

Preserve policy, durable audit, MCP/agentd, owning-connection identity, server
cancellation, verified TLS, atomic exports, Redis, MongoDB, and optional DuckDB.

## Upstream disposition: all 60 Linux commits

[Pinned comparison](https://github.com/TableProApp/TablePro/compare/807d280944a9c9493b7ebaf2224ad993ce0c4c52...5730238f5c72b4924669efbb7c0e79372de50a36).

| Commits | Changes and decision |
| --- | --- |
| `3c596b066`, `ca95de3ec`, `3a96a7c15`, `df6c57207`, `dc95e8f80` | Copy/export, ClickHouse quoting, textual CSV protection, CSV library, precision: converge serializers, retain atomic/cancellable exports |
| `5bd1716e9`, `511df958b`, `9e32d5b8e`, `4b081374a`, `b6aec666b`, `dc07edd56`, `2bc2e1008`, `f85964342` | Cleanup, tiberius, gettext, russh, Rust 1.98, crypto, optional Kerberos, SQLx 0.9/system SQLite: B1 with transport/optional-driver checks |
| `2273baa64`, `75d2fdd8f`, `3f382d978`, `cb2348df6`, `52b0315de`, `f32d44ae0`, `ddd3cadb0`, `4a58098da` | Library split, identity, GResource/Meson, GNOME 50, shortcuts/icons, gettext/logging: adopt structure/platform with approved identity exceptions |
| `47c5b90e1`, `088a034ee`, `b9cba9fff`, `3ba64e756`, `5730238f5` | Nextest, GTK helpers, CI simplification, fixtures, schema rebuild/SQLite dev environment: adopt conventions, retain our safety/release/soak gates |
| `4ae0bfb1e`, `00c8c650d` | GSettings/preferences/window/font: adopt with migration and rollback |
| `6cd0209d4`, `057478973`, `d81e1e055`, `5d1c9d496`, `203804a44` | Paths/private durable files/history migrations/unreadable connections/keyring/isolated stores: adopt ownership, retain stronger validation/locking/unknown-field retention |
| `ae733dba6`, `ee82e56e6`, `10650cabf`, `cf9abd789`, `aa84cb92f` | Runtime/tasks/coalesced persistence/session-health: adopt with audit-aware lifecycle |
| `33b5ec872` | Complete drafts: bring forward to 0.1.1 |
| `a4acba78f`, `21a05fb28` | Enter submits/defaults/secret reporting: 0.1.1 fixes; preserve user-entered ports |
| `1fb43c9c9` | History retention/clearing: retention already reread by our timer; adopt service ownership in B2 |
| `75ad43753`, `97b04de5a`, `e685a042b`, `26ea9fe24` | Endpoints/TLS/credentials/OpenSSH/saved SSH/network: integrate GUI/daemon with service identity and legacy compatibility |
| `793ee3456`, `0572de7d6`, `7e120bc94`, `e2661d450`, `96ecbcb35`, `6c100ec40`, `400aade12` | Lossless values/typed columns/statements/results/metadata/input/errors/decoding: B3 across all consumers/eight drivers |
| `f483e4169`, `50a515a36`, `933c5ad95` | Script planner/safe formatting/current statement: reuse upstream; preserve existing shortcuts/run-at-cursor |
| `f5a53e4bb`, `eab55459f` | Typed writes/filters: adopt behind policy/approval/audit |
| `83545baab`, `d47514b4a` | Owned cancellation/write outcomes/session capabilities: integrate; durable audit remains authoritative |

Lossless decoding is integrated upstream; several session/OpenSSH pieces remain
preparatory. Importing types alone is not completion. russh 0.62.7 is the current
floor: 0.60.3 covered [0153](https://rustsec.org/advisories/RUSTSEC-2026-0153.html)
and [0154](https://rustsec.org/advisories/RUSTSEC-2026-0154.html) only. Later GHSA
client panics and parser DoS need 0.62.4 or newer.

### macOS lessons

- Adopt array modifiers/delimiters (`96b2c057b`, `fb94495d0`); preserve declared types.
- Keep original-row identity regressions (`476d99ce9`, `bbc5ad2b6`, `3f2bf37ef`).
- Stable completions (`cd5ed11a4`) are substantially covered by sorted/deduplicated candidates.
- Validate SQL Server options (`30286aa7a`) on tiberius before copying FreeTDS behavior.
- Catalog acceptance: empty databases, per-database metadata and native capabilities
  (`77e4f524e`, `52a7181e6`, `c602a9aa5`, `31901104a`).
- Defer schema mutations/privileges, credential profiles, AWS discovery, remote
  SQLite, cross-engine transfers, advanced array editors and diagrams.
- Exclude Apple frameworks/UI, Sparkle, iOS sync, licensing and plugin ABI.

## Upstream feature adoption, September 2026

Eleven of the thirteen features the [2026-09-19 re-survey](upstream-sync.md) found
missing are merged; the other two already existed here in a different shape. The work
ran as five parallel worktree branches and is recorded in `CHANGELOG.md` under
`[Unreleased]`.

It is not verified. No new GTK surface has been rendered, so
[manual-verification-0.2-features.md](manual-verification-0.2-features.md) is the gate
between this and any release candidate. B3 and B4 follow after that.

## Ordered packages

One primary implementation stream. Small reviewable commits with upstream
references, tests, and differences. Implementation and release verification differ.

### A: BookiE 0.1.1

- [x] **A1 correctness**: reproduce/fix false-NULL decoding, SSH secret-save
  reporting, ordered settings persistence. Failed decoding never becomes editable/
  exportable NULL. Preserve unreadable files and last durable state.
- [x] **A2 editor persistence**: complete drafts, upstream script planning and
  token-safe formatting. Read old inline drafts; persist new files before references.
  Save failures remain visible/recoverable.
- [x] **A3 Jump to Column**: search loaded browse/result metadata by source ordinal
  and generation, including duplicate/hidden/reordered columns. Reveal/scroll/focus
  without data changes. Keyboard and stale-picker tests.
- [x] **A4 identity**: BookiE/original branding, `bookie`/`bookie-agentd` with old
  command compatibility. Arch `bookie` replaces/conflicts with `tablepro`. Keep app
  ID, paths, keyring schema, UUIDs, audit format and protocol contracts.
- [ ] **A5 candidate**: explicit candidate SHA/version packaging without a published
  tag; build/install/upgrade/rollback/soak. Keep repository and `linux-v…` tags.
  Publishing remains an explicit separate decision.

### B: BookiE 0.2.0

- [ ] **B1 platform/build**: Rust 1.98, GNOME 50, SQLx 0.9/system SQLite, crypto/
  dependencies, Meson/GResource, library entrypoint, gettext/logging, isolated dev
  profiles, Arch/development Flatpak. Keep internal crate names when renaming adds
  no compatibility benefit. GNOME 50 (`gtk4` `gnome_50`, `libadwaita` `v1_9`,
  `sourceview5` `v5_18`, `glib`/`gio` `v2_88`) is enabled after migrating the
  shortcuts dialog and calendar API. Cargo compile and Clippy pass on the host;
  the development Meson build/install path also passes. Flatpak execution and
  installed-package qualification remain.
- [ ] **B2 runtime/storage**: owned Tasks, explicit stores, private durable writes,
  history migrations, GSettings, coalesced writers. Remove replaced globals/runtime
  calls; flush persistence and settle governed operations at shutdown.
- [ ] **B3 lossless contracts**: upstream values, columns/results, errors, typed
  dialect/filter/write, metadata/row identity. PostgreSQL/MySQL/SQLite/SQL Server/
  ClickHouse/Redis/MongoDB/optional DuckDB. Exact values through every consumer.
- [ ] **B4 transport/sessions**: network/OpenSSH/session integration in GUI/daemon,
  policy/audit guarded. Dedicated editor sessions, cancellation, transaction ownership,
  terminal outcomes/retirement. No weaker auth/TLS fallback.
- [ ] **B5 editor/files**: shared-planner highlighting, existing shortcuts, GTK/GIO
  Open/Save/Save As, dirty prompts/external-change detection. Open creates a tab;
  atomic save rejects stale versions until reload/overwrite/Save As is selected.
  Draft recovery stays independent of file saving.
- [ ] **B6 PostgreSQL catalog**: read-only schemas, views/materialized views,
  routines, triggers, sequences, extensions, roles/grants and enum/composite/domain/
  range types. Reuse upstream structures. Guarded `list_objects(kind, schema)` and
  `list_types(schema)` shared by GUI/MCP; unsupported/denied/failed/empty differ.
- [ ] **B7 qualification**: reconcile roadmap, attribution, compatibility ledger,
  release notes. Freeze/verify 0.2 independently of 0.1.1 evidence.

Update policy wrappers/agentd forwarding alongside internal contracts. MCP methods
and scopes stay compatible; catalog tools are additive. Separate wire/persistence
adapters; never rewrite audit history. Before preferences/history/workspace format
changes, create private backups, migrate transactionally/idempotently, retain legacy
data until durable, and test documented restoration on package rollback.

## Schedule, models and verification

September 16–29 checkpoint: A1–A4 and target frozen 0.1.1 candidate, then verification.
B1 → B2 → B3 → B4; B5/B6 follow prerequisites; B7 last. Allow 6–9 weeks for one
engineer with AI, an estimate rather than guarantee. Extend dates, not hidden scope cuts.

Terra medium: bounded fixes, Jump to Column, branding, file workflows, packaging/docs.
Sol medium: decoding, drafts/planner, platform, runtime/storage, catalog. Sol high:
lossless contracts and transport/sessions. Astra medium: B4 architecture/security
review, final risk review, unresolved integrations. Recommendations, not measured
benchmarks: [OpenAI models](https://developers.openai.com/api/docs/models/compare).

Each task packet records start SHA, references, behavior, preserved contracts,
acceptance commands, exclusions, resulting SHA and actual checks. Do not label
uncommitted work as a resulting commit.

- Correctness: wide numerics, NULL/decode failure, binary/JSON/temporal values,
  arrays, generated columns, stale identities and exact exports.
- Editor/storage: >256 KiB, Unicode, restart, disk failure, malformed files,
  ordered rapid saves, cancelled dialogs, external edits.
- SQL: token equivalence, dialect boundaries/comments/delimiters, parameters,
  read-only/admin classification.
- Transport: original TLS name through SSH, key changes, jumps, auth methods,
  subprocess cleanup, cancellation/reconnect/ambiguous writes.
- Catalog: hostile identifiers, restricted roles, unsupported engines, cancellation,
  DDL refresh, stale results, GUI/MCP agreement.
- Candidate: fmt, Clippy, audit/deny, default/optional builds, relevant driver/TLS/
  PostgreSQL/Secret Service/GTK, package validation, Wayland install/upgrade/rollback.
- 30 consecutive retry-free GTK attempts across six or more runs at one SHA.
  Candidate changes invalidate affected evidence.

## Documentation and boundaries

Link this sprint from PLAN.md/ROADMAP.md and mark the 0.1.1 plan superseded while
retaining history. Keep shared Rust close to upstream, import with attribution,
isolate branding/governance/compatibility/additional drivers. Never merge Apple
source. Prepare upstream-compatible fixes separately. Contacting authors, upstream
PRs, publication and repository rename are outside this task's authorization.

Status on 2026-09-25: A1–A4 are implemented and ticked; their evidence is from
the 0.1.2–0.1.4 work, not from a frozen 0.2 candidate. A5 and B1–B7 stay open.
B1 lacks a real Flatpak build and installed-package checks. B2 lacks the
GSettings and history-format migrations. B5 has Open, Save, Save As, a
changed-on-disk check before every save and a close prompt; a tab's file
binding is not yet restored after a restart (its text is, through drafts). B6 lists PostgreSQL views only. B3 and B4 have not
started.

Deferred beyond 0.2: administration mutations, bulk import/export/backup/restore,
new engines, all-connection/window restoration, dashboards, built-in AI.

## Implementation ledger

- 2026-09-16: approved plan saved at baseline `143a389ae`; no application changes
  or new release verification recorded yet.
- 2026-09-16 working tree: PostgreSQL false-NULL regression failed against the
  baseline on a real PostgreSQL container, then passed after the fix. It also
  exposed the SQLx chrono infinite-date panic; checked temporal decoding now
  rejects unsupported ranges without panicking. All 10 PostgreSQL integration
  tests passed. SSH save errors now propagate with recovery instructions.
- Imported upstream script planner and token-safe formatter. Core/app library and
  binary checks passed (502 tests) before the subsequent draft-storage changes.
  Ordered/coalesced settings persistence has focused concurrency/error tests.
  Full candidate qualification, GTK integration and milestones remain pending.

- 2026-09-16 working tree: complete draft files and legacy backup migration passed
  21 focused workspace tests and an actual GTK restart using a >256 KiB Unicode
  query. Imported planning is now connected to script execution and run-at-cursor.
  `GO n` is explicitly rejected rather than silently ignoring its repeat count;
  scripts continue to stop after the first error.
- Jump to Column passed its GTK keyboard scenario (duplicate names, Unicode search,
  Enter, no-match handling, Escape) and an isolated GTK stale/hidden/reordered
  target test with fatal criticals. BookiE branding, original icon, command aliases,
  Arch replacement metadata and candidate SHA/version packaging are implemented.
  ShellCheck, desktop/AppStream validation, makepkg metadata and candidate archive/
  rejection tests passed. No package has been published or installed on the host.
- Workspace unit run: 872 passed before the final planner integration; app rerun:
  247 passed, one isolated-display test separately exercised. Clippy and repository
  file-size/panic/operation-bound guards passed. Full GTK suite is in progress.

### Workspace rollback procedure

Stop BookiE before changing packages or restoring files. Preserve a private copy of
`$XDG_CONFIG_HOME/tablepro` and `$XDG_DATA_HOME/tablepro` (defaults `~/.config` and
`~/.local/share`), including the new drafts directory. For rollback to the prior
package, restore `workspace_state.before-drafts.json` as `workspace_state.json` in
the config directory. This restores the pre-migration workspace; newer draft SQL
remains in the data directory for manual recovery. Do not delete draft files or
keyring records during package rollback. A migration fixture verifies original
backup bytes, but an installed-package rollback is still a release gate.

- The full 19-scenario GTK suite passed after correcting the harness to wait for
  the file chooser's Save button and editable control, rather than matching the
  still-closing export dialog by title alone. `scripts/ci-local.sh full` passed
  formatting, Clippy, workspace unit tests and the sandbox integration tier.
- Refreshed dependency checks found [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html)
  in Rustls 0.23.43. The patch update to 0.23.45 is part of 0.1.1; transport and
  advisory checks must be rerun. Rust 1.98.1 is installed for the later B1 work.
- 2026-09-17: the GNOME 50 platform bump (`gtk4` `gnome_50`, `libadwaita` `v1_9`,
  `sourceview5` `v5_18`, `glib`/`gio` `v2_88`) was built and Clippy-checked inside a
  `debian:testing` container (the only readily available Linux environment with
  glib >= 2.88 and libadwaita >= 1.9; `ubuntu:25.10`, the CI GTK image, ships glib
  2.86 and libadwaita 1.8 and cannot satisfy the bump at all). The build itself
  succeeds, but `cargo clippy -- -D warnings` fails with 40 errors: GTK4's
  `ShortcutsWindow`/`ShortcutsSection`/`ShortcutsGroup`/`ShortcutsShortcut` builder
  API is deprecated wholesale as of 4.18 (`crates/app/src/ui/app/shortcuts.rs`,
  ~15 call sites), and `Calendar::select_day` is deprecated as of 4.20
  (`crates/app/src/ui/grid/editing.rs:220`). Reverted just the four feature-flag
  lines in the workspace `Cargo.toml` back to their pre-bump values (crate
  *versions* did not change, only which deprecated API surface compiles in), so
  `linux` builds and lints clean again. The underlying GNOME 50 migration is real,
  wanted B1 work — it needs the Shortcuts window rebuilt against its replacement
  API (own GTK release notes have not been checked yet) and the one Calendar call
  site updated, verified visually before re-enabling the feature flags. Everything
  else from this pass (Rust 1.98, SQLx 0.9/system SQLite, tiberius fork switch,
  russh 0.63, BookiE identity/Meson scaffolding) is unaffected and already in
  `linux`. Also added `libsqlite3-dev` to every CI job that builds the sqlite
  driver or the app crate — missing before, and a certain CI failure once system
  SQLite linking landed, independent of the GNOME 50 question.
- 2026-09-16/17: reviewed the macOS `main` branch's Swift test suite (~1,300
  files) for adoptable test cases against `linux`'s own logic, tracked in
  [docs/testing.md](testing.md)'s "Upstream test-suite parity" section.
  Ported 5 items — SSH (host-key/socket-path/connect-error edge cases),
  cell/value display (binary edge cases; surfaced but did not fix an
  `is_cell_editable` type-affinity gap needing GTK visual verification), row
  copy/paste (TSV escaping, paste normalization), storage (found already
  ahead of main's equivalent suite; one real gap in query-history export
  formatting), and Redis (reply-to-value mapping, error classification) — 34
  new tests, workspace suite 879 → 911. A follow-on mutation-testing pass
  (`cargo mutants` against the newly touched files, not yet wired into CI at
  the time) found 3 tests that passed without actually pinning the behavior
  they claimed to: a boundary check whose test couldn't reach the exact edge
  (fixed by extracting the check into its own pure function), a CLI-escape
  guard tested on only one side of its condition, and a constant referenced
  only symbolically so a change to its own definition couldn't be caught.
  All three fixed and reverified; `linux-quality.yml`'s scheduled mutation
  job now also covers `ssh` and `driver-redis` going forward. Bumped MSRV
  and CI to Rust 1.98 in the same pass, validated against the real toolchain
  locally (fmt/clippy/929 tests/`cargo deny` clean) before landing.

### Local implementation commits

| Commit | Package | Evidence / differences |
| --- | --- | --- |
| `16b66e4` | A1 PostgreSQL | Ten real PostgreSQL integration tests; decoding failure is an error, not NULL |
| `d3e5294` | A2 core import | Pinned upstream grammar/planner; no Apple tree import |
| `b84f3f8` | A1/A2 persistence | Coalescing/error tests, full drafts, byte-preserving backup, GTK large-script restart |
| `df31227` | A2 editor | Token-guarded formatting and planned execution; policy/audit unchanged; GO repetition explicitly rejected |
| `c6e5507` | A3 grid | Keyboard, duplicate/Unicode names, stale/hidden/reordered targets; empty-result metadata retained |

The above checks used the evolving working tree. They are implementation evidence,
not the exact-candidate soak ledger. No public release or upstream contact occurred.

### 2026-09-17 refreshed fork baseline

Fetched `origin` and fast-forwarded cleanly from `32ce81f9c` to
`e9bba1f5b24575b4eb959b492421ff50c9117063` (nine incoming commits). Reviewed
the latest 30 commit subjects/change inventories and inspected the safety-critical
diffs and current callers. The [review ledger](baseline-review-2026-09-17.md)
records coverage and unresolved findings.

A1–A4 implementation is present and strengthened by the 0.1.2–0.1.4 work. A5
qualification remains separate from implementation and package/version labels.
B1 is partially integrated; do not redo SQLx/Rust/library/resource imports. The
GNOME 50 API migration, development-profile isolation and Meson development build
are now present, while Flatpak/package qualification remains. B2–B4 must retain runtime
cell validation, formatter adjacency checks, material-sensitive session retirement,
SCRAM caps, service-host TLS identity, and the executable isolated-test inventory.

This pass removed stale duplicate embedded artwork, reusing the packaged SVG
through a GResource alias. Resource compilation and byte-for-byte extraction
passed. Core formatter tests: 16 passed on Rust 1.98. Arch candidate tests and
isolated-test inventory passed. Debian package fixture could not run because
  `dpkg-deb` is absent. The later verification below supersedes this pass's limited
  evidence. Candidate soak, package installation and publication remain open.

- 2026-09-17 continuation: re-enabled the GNOME 50 feature floor after replacing
  deprecated GTK shortcut widgets with `AdwShortcutsDialog` and `Calendar::select_day`
  with `Calendar::set_date`. App all-target check and Clippy passed on GTK 4.22.4,
  libadwaita 1.9.3, GtkSourceView 5.20.0 and GLib 2.88.3. Temporary Meson/Ninja tools
  built and installed the development profile; generated `.Devel` desktop/AppStream
  metadata and command aliases validated. Storage paths and Secret Service schemas
  now derive from the build profile; production retains its legacy identity and
  development uses `tablepro-devel`/`com.tablepro.linux.Devel.Password`. Storage
  tests passed (95, two ignored), plus the focused development-profile identity test.
  CI images and the production/development Flatpak manifests now target GNOME 50;
  a real Flatpak build remains required.

- 2026-09-17 second-review pass: independently re-verified the working tree above
  (not just the review doc's claims) against the actual diffs, then closed part of
  the "verification gap" it flagged. The MongoDB SCRAM patch guard
  (`patched_mongodb_caps_scram_iterations`) only checked source text, not behavior;
  added 3 direct tests against the vendored `ServerFirst::validate` (prefix match,
  mismatch, and a client nonce longer than the server's echoed nonce — the exact
  shape that would have panicked under the old unchecked slice index). Could not
  execute these locally: the vendored crate's dev-dependencies require its original
  upstream workspace, which isn't present here; verified correct by tracing the
  logic instead. The equivalent `sqlx-postgres` SASL iteration-count guard
  (`patched_sqlx_postgres_caps_scram_iterations`) has the same weakness but was left
  alone after confirming isolation is a bigger job there — `sqlx-postgres` depends
  on `sqlx-core` throughout the crate, and `sqlx-core` is itself workspace-versioned,
  so testing it standalone would mean reconstructing much of the upstream sqlx
  workspace, not a small change.
  Ran `cargo mutants` against the two newest security-relevant files (redirecting
  its scratch build directory off `/tmp`, which was full from the development
  Meson build above). `transport::session_material`: 16/27 → 23/27 caught. Added a
  hardcoded value test for `MAX_MATERIAL_FILE_BYTES` (was only referenced
  symbolically), a test proving the size limit is `>` not `>=`, and a test proving
  hop-0 password SSH auth is not wrongly refused by the jump-hop guard (only hop 1+
  refusal was tested). Left 2 mutants open: one needs a mocked Secret Service, one
  needs a real TOCTOU race — both match this document's B4 note above, not new gaps.
  `ssh::lib`: 10/65 → 16/65 caught. The GNOME-50-adjacent socket-path fallback
  (try `XDG_RUNTIME_DIR`, then `/tmp`) had only a happy-path test; extracted the
  boundary arithmetic into a pure `socket_dir_path_fits` function and tested it at
  the exact limit. Remaining misses all need a live SSH server/socket
  (multi-hop chain indexing, `Drop` impls, `connect_and_auth`) — integration-tier,
  consistent with earlier assessment of this file.
  Full workspace: fmt clean, Clippy 0 errors (2 pre-existing informational warnings
  inside `vendor/sqlx-postgres`, outside workspace-member lint scope), 952 tests
  passing (up from 947), `cargo deny check` clean. These security changes landed as
  `2a3b8c7` (`fix(linux): harden authentication and session material`).

- 2026-09-17 B1 checkpoint: GNOME 50 APIs, isolated development storage/keyring,
  Meson/GResource metadata, development Flatpak manifest and CI floors landed as
  `399fb5b` (`build(linux): complete GNOME 50 development baseline`). On the exact
  combined tree, preflight, `scripts/ci-local.sh full`, and `cargo deny check`
  passed. The staged Meson development install passed all 19 GTK safety scenarios;
  the harness now selects `tablepro-devel` explicitly for development builds.
  A real Flatpak build, installed package upgrade/rollback and candidate soak remain.

- 2026-09-17 B2 persistence slice: `b7719b4` removes the process-global column-width
  and filter stores. The application opens them once, passes them to every window
  and browse tab, and flushes the shared ordered/coalescing writers at shutdown.
  Unreadable-file preservation and production file formats remain unchanged. App
  check, 263 library tests, Clippy and all 19 GTK safety scenarios passed. History,
  workspace and database-service globals remain for later B2 slices.

- 2026-09-17 B2 history slice: `2c80851` replaces the process-global query-history
  pool with an application-owned `HistoryStore`. The same explicit store is passed
  to editors, the history dialog, preferences, and scheduled retention work; the
  existing database path and format remain unchanged. Preflight, app and storage
  tests, Clippy, and all 19 GTK safety scenarios passed. Workspace and
  database-service globals remain for later B2 slices.

- 2026-09-17 B2 workspace slice: `bc701a3` replaces the global workspace
  cache, locks, and writer with one application-owned `WorkspaceStore` shared
  by every window. Existing draft/workspace formats, coalescing, retry, and
  close-time flush behavior remain unchanged. The app suite (260 passed, 3
  isolated-GTK tests ignored), a focused ownership test, Clippy, and all 19 GTK
  safety scenarios passed. Database-service and preference globals remain for
  later B2 slices.

- 2026-09-17 pre-release code review: reviewed the diff against this sprint's
  `143a389ae` baseline (96 files, `crates/`) for correctness, robustness, and
  compliance with the repository rules. Fixed 8 confirmed bugs, each with a
  regression test: the ClickHouse heredoc lexer arm silently dropped its
  unterminated-quote diagnostic; `ScriptPlan::statement_at` mapped "cursor is
  before any statement" to statement 0 instead of `None`; "Run at cursor"
  refused to run a clean statement if an unrelated unterminated construct
  existed anywhere else in the buffer; one connection's unreadable editor
  draft aborted workspace restoration for every connection at startup instead
  of only its own tab; deleting a saved connection left its on-disk editor
  drafts behind indefinitely; ClickHouse connection timeouts and MongoDB
  server-selection failures (the most common outcome for an unreachable host)
  were misreported as certificate hostname mismatches; and `agentd`'s
  `open_session` fetched fresh session material (Secret Service plus file
  reads) before checking the connection cache, so a transient lookup failure
  hard-failed calls even with a healthy cached connection available. Full
  workspace fmt, Clippy, `--lib --bins` tests, the two named `tablepro-mcp`
  integration tests, the sandbox tier, and `cargo deny check` all passed;
  Docker-gated driver integration tests were not run (no container runtime
  available in that session).

  Two items were reviewed and deliberately left open for later, scoped work
  rather than a rushed fix: `ScriptPlan::batch_error_policy()` (and MySQL's
  `ContinueNextBatch`) is computed but has no caller — `run_statements` in
  `crates/app/src/ui/editor/outcomes.rs` always stops a script on its first
  statement error regardless of driver, so continue-after-batch-error is
  currently inert and needs batch-boundary-aware plumbing from the editor's
  run path. And Postgres `collect_query_rows` aborts an entire result set if
  any single cell fails to decode; the previous silent-NULL behavior was
  itself a deliberately fixed bug (masking bad data as absent data), so the
  real fix is a new "undecodable cell" value representation threaded through
  `tablepro-core` and the grid UI, not a quick patch. Lower-severity cleanup
  also noted: a second hand-rolled background writer in
  `workspace_state.rs` and comments in `app/src/lib.rs`, `logging.rs`,
  `sql_format/mod.rs`, and `storage/query_history.rs`. Both were reviewed
  on 2026-09-18 and closed without the changes the note assumed: the
  workspace writer merges per-connection entries under a file lock and is
  not a `StateFile<T>` duplicate (see
  [state management](state-management.md)), and the comments record
  external parser and engine behaviour, which the repository rule now
  allows explicitly. No release was tagged or built in this pass;
  A5 and B3–B7 remain open as tracked above.

- 2026-09-17 follow-up: closed the batch-error-policy item the review above
  left open. `ScriptPlan::batch_error_policy()` is SQL Server's `GO`-batch
  setting, not MySQL's (this document's own note above mislabeled it); each
  `GO`-delimited batch was already one planner "statement," so no
  batch-boundary plumbing was actually needed. `script_statements` now
  returns the policy alongside the statement list, and `run_statements` only
  stops the script on the first error when the policy is `StopScript`; under
  `ContinueNextBatch` a failed batch no longer prevents later batches from
  running. Regression tests cover both policies in `outcomes.rs` and the
  parser-level policy value in `statement_cursor.rs`. Full workspace fmt,
  Clippy, `--lib --bins` tests, the two named `tablepro-mcp` integration
  tests, the sandbox tier, and `cargo deny check` all passed.

- 2026-09-17 follow-up: closed the Postgres undecodable-cell item the review
  above left open. `extract_value`/`collect_query_rows` no longer aborts an
  entire result on one undecodable cell: added
  `tablepro_core::Value::Undecodable`, carrying the column's type name,
  threaded through every consumer that matches on `Value` (core
  export/SQL-literal rendering, every driver's write bind path, MCP JSON
  serialization, the change tracker's row-identity key, the activity dialog,
  and the grid). The grid marks a cell holding this variant read-only, the
  same as `Bytes`; CSV/JSON/SQL-literal export show it as an explicit
  `<undecodable TYPE>` marker instead of silently rendering NULL or empty.
  Write-path binds treat it as unreachable from real user input (the grid
  never lets it be edited) and fall back to writing NULL, except
  ClickHouse's already-fallible literal renderer, which now returns a real
  error instead. Updated the Docker-gated Postgres integration test that
  previously asserted the old "whole query fails" behavior to assert the new
  per-cell degradation instead. Full workspace fmt, Clippy (including the
  optional `tablepro-driver-duckdb` crate, which needed the same write-path
  match arm), `--lib --bins` tests, the two named `tablepro-mcp` integration
  tests, the sandbox tier, and `cargo deny check` all passed; Docker-gated
  driver integration tests were not run (no container runtime available in
  this session).

- 2026-09-17 test hardening: strengthened the two follow-up fixes' unit
  coverage. `BatchErrorPolicy`: `script_statements` is now checked against
  every grammar (not just Postgres/MSSQL) so a future grammar addition
  defaults to `StopScript` unless explicitly opted in, plus the unrecognised-
  driver fallback path; `run_statements` gained cases for more than one error
  in a row and for a parameter-bind error (not just a driver error) under
  `ContinueNextBatch`. `Value::Undecodable`: added a `KeyValue` round-trip
  test and a distinct-row-identity test (two undecodable PK values with
  different type names must not collide) in the change tracker, an MCP
  `value_to_json` test asserting the JSON shape is never `null`, and a grid
  `cell_text_for_bind` test confirming an editable column still shows the
  undecodable marker rather than the empty-editable-NULL text. Full workspace
  fmt, Clippy, and `--lib --bins` tests (23/23 binaries, 0 failed) passed.

- 2026-09-18 mutation testing: ran `cargo mutants --in-diff` (scoped to the
  two follow-up fixes' changed lines) against the workspace, redirecting its
  scratch build off `/tmp` (a 6.8 GB tmpfs here, too small for a full
  from-scratch workspace rebuild) to a disk-backed `TMPDIR`. Every mutant on
  lines the follow-up fixes actually added or changed was caught: the
  `stops_on_error` comparison in `run_statements`, the parser guard in
  `script_statements`, and every `Value::Undecodable`-handling function
  across `export.rs`, `sql_literal.rs`, `grid/display.rs`, ClickHouse's
  `literal`, MSSQL's `boxed_params`, and MCP's `value_to_json`. The diff also
  swept in the unrelated `DatabaseService` ownership refactor (same commit
  range), which produced many additional "missed" mutants in GTK
  `update`/`init` methods that merely gained a new parameter; those are
  pre-existing structural gaps in GTK-only code this project already
  verifies manually, not new holes.

- 2026-09-18 B2 close-out: converted the last two process-global singletons.
  `crates/app/src/services/database_service.rs`'s `static SERVICE: OnceLock`
  is now built once in `lib.rs::run()` as `Arc<DatabaseService>` and threaded
  explicitly through every GTK component and free function that used
  `database_service::instance()` (34 original call sites, plus the cascade
  through `CatalogOrigin`/`CatalogChanges`, `PreparedConnection::activate`,
  and the `BrowseTab`/`SqlEditor`/`HistoryDialog` component `Init` structs —
  24 files total). `services/preferences.rs`'s `static CACHE` became a
  `PreferencesStore` handle, threaded the same way through 17 files,
  including `operation_control::configured_timeout_secs`'s own 19 call
  sites. `services/mcp_service.rs`'s `static BRIDGE: OnceLock<Arc<McpBridge>>`
  is now the `Option<Arc<McpBridge>>` `start_background` already returned,
  threaded through `AppInit`/`App` into the MCP preferences page instead of
  being reached globally. No behavior changed in any of the three; each
  landed as its own commit with full workspace fmt, Clippy, `--lib --bins`
  tests (23/23 binaries), the two named `tablepro-mcp` integration tests, and
  the file-size guard passing. B2's "remove replaced globals" goal is now
  complete; GSettings migration and history-format migration (also named
  under B2) were out of scope for this pass and remain open, so the
  checklist item stays unmarked until qualified. B3–B7 and A5 remain open.

- 2026-09-25: re-surveyed upstream `main` from `fc8b887af` to `e6678a93b`
  (189 commits, v0.72 to v0.75.0); upstream `linux` has nothing after
  `0e542e3c1` (2026-09-18), already covered on 2026-09-19. Ported four fixes
  whose defect class existed here, each with a regression test that failed
  first: SQL Server batches are read to the end so later errors surface and
  a row-limited batch finishes on the server; PostgreSQL indexes keep
  expression keys and partial predicates and drop INCLUDE columns; SQLite
  foreign keys to an implicit parent key name the key columns. The SQLite
  work also found that a bare catalog PRAGMA can answer from a pooled
  connection's stale schema; catalog reads now use table-valued pragmas.
  The GUI policy file now follows the build profile. Container suites for
  SQL Server, PostgreSQL and SQLite passed; GTK tiers were not run.
