# BookiE 0.1.1 → 0.2: upstream convergence and daily workflows

Approved: 2026-09-16. Status: implementation started, no release approved.
Delivery branch: `linux`, `origin/linux` in `cozyGarage/TablePro` (called `fork`
in older documents). This document supersedes the 0.1.1 plan for sequencing.

## Goal and baseline

Deliver BookiE 0.1.1 first, then 0.2.0 with the full upstream Linux foundation,
daily editor/grid workflows, and read-only PostgreSQL catalog/type browsing.
Keep a two-week checkpoint; extend the sprint until both releases pass their gates.

| Source | Pin | Review |
| --- | --- | --- |
| Our Linux | `143a389ae2b9e5abc13306d7c6f22299a94a6fb7` | Current implementation and plans |
| Upstream main | `fc8b887af7cf2b2e72c5d01f5b5b5c952c93f4e3` | 74 commits after `035ffe8f`, targeted source inspection |
| Upstream Linux | `5730238f5c72b4924669efbb7c0e79372de50a36` | All 60 subjects after `807d28094`, targeted source inspection |

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
preparatory. Importing types alone is not completion. Our russh/cryptovec 0.60.3
already satisfy [0153](https://rustsec.org/advisories/RUSTSEC-2026-0153.html) and
[0154](https://rustsec.org/advisories/RUSTSEC-2026-0154.html); upgrading is alignment.

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

## Ordered packages

One primary implementation stream. Small reviewable commits with upstream
references, tests, and differences. Implementation and release verification differ.

### A: BookiE 0.1.1

- [ ] **A1 correctness**: reproduce/fix false-NULL decoding, SSH secret-save
  reporting, ordered settings persistence. Failed decoding never becomes editable/
  exportable NULL. Preserve unreadable files and last durable state.
- [ ] **A2 editor persistence**: complete drafts, upstream script planning and
  token-safe formatting. Read old inline drafts; persist new files before references.
  Save failures remain visible/recoverable.
- [ ] **A3 Jump to Column**: search loaded browse/result metadata by source ordinal
  and generation, including duplicate/hidden/reordered columns. Reveal/scroll/focus
  without data changes. Keyboard and stale-picker tests.
- [ ] **A4 identity**: BookiE/original branding, `bookie`/`bookie-agentd` with old
  command compatibility. Arch `bookie` replaces/conflicts with `tablepro`. Keep app
  ID, paths, keyring schema, UUIDs, audit format and protocol contracts.
- [ ] **A5 candidate**: explicit candidate SHA/version packaging without a published
  tag; build/install/upgrade/rollback/soak. Keep repository and `linux-v…` tags.
  Publishing remains an explicit separate decision.

### B: BookiE 0.2.0

- [ ] **B1 platform/build**: Rust 1.98, GNOME 50, SQLx 0.9/system SQLite, crypto/
  dependencies, Meson/GResource, library entrypoint, gettext/logging, isolated dev
  profiles, Arch/development Flatpak. Keep internal crate names when renaming adds
  no compatibility benefit.
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
