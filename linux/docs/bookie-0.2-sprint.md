# BookiE 0.1.x → 0.2: upstream convergence and daily workflows

Approved: 2026-09-16. Status: implementation started, no release approved.
Delivery branch: `linux`, tracked here as `fork/linux` in `cozyGarage/BookiE`. This document supersedes the 0.1.1 plan for sequencing.

## Current continuation plan: 2026-09-29

Working branch: **`linux`**. Reviewed tip:
`fe22716459c242b350bf34e7070dde19bba184ba` (source version 0.1.5).
Target: **0.2.0**, implementation in progress; no release approved by this review.
The [archived review](sprint-review-2026-09-28.md) inventories all 117 reachable
commits in the September 26–28 window and records delivered work and open risks.
The September 29 follow-ups are recorded below and in the linked type/value
contracts; B3 remains open.
Earlier dated reviews and implementation entries below remain historical evidence.
This section supersedes their next-task order and desktop-target requirements.

### Priority and desktop scope

1. **B3:** finish the remaining value and consumer contracts, one reproducible case
   per task. Reuse the existing corpus, engine fixtures and change-contract runner.
2. **B4:** finish the open transport, policy, daemon and GUI tasks on the
   [task board](b4-task-board.md). A completed lane does not close the milestone.
3. **Arch Linux / Omarchy / Hyprland / native Wayland UI:** verify the resulting
   connection, session, grid and editor workflows, then package/upgrade/rollback.
   The existing B5/B6 implementations enter through this acceptance pass.
4. **GNOME desktop on Debian / native Wayland UI and packaging:** the required
   next phase after the Arch pass. Complete Debian recipe task I1, installed
   workflows and upgrade/rollback before closing desktop qualification.

GTK/libadwaita and the already integrated GNOME 50 API/library requirements remain
part of the build on Arch. The GNOME/Debian phase follows Arch in the same plan.
Keep existing cross-platform Linux CI checks; environment checks scheduled after
Arch remain pending, not passed. Flatpak compatibility decision I2 remains a bounded B4 task;
full Flatpak release qualification follows later packaging work. No GNOME/Debian
VM setup is a prerequisite for current B3/B4 implementation or the Arch UI pass.

### Current milestone status

| Milestone | Status at review tip | Next acceptance |
| --- | --- | --- |
| B1 | Platform foundation integrated; source version is 0.1.5 | Installed Arch candidate first, then required Debian/GNOME qualification; full Flatpak qualification remains separate |
| B2 | Implemented; retain completed code and migration safeguards | Installed Arch upgrade/rollback in B7 |
| B3 | Substantial eight-driver and export coverage; still open | Remaining native-type targets, exact editing, consumer parity and mutation triage |
| B4 | A/B, C1–C5, D, G1/G2/G4 and H have recorded implementation/regression evidence | C6, E, F, G3/G5 and active I tasks; A5 monitor follow-up F9 |
| B5/B6 | Editor files and read-only PostgreSQL catalog implemented | Arch Wayland file-dialog/recovery and catalog stale/restricted-role acceptance |
| B7 | Open; this review is documentation only | Freeze SHA, affected automated gates, installed Arch then Debian/GNOME acceptance and retry-free soak |

### B3 work packets for Luna

Start with **B3-P1**, then take one bounded case from P2–P5. Split a packet by
engine/type/consumer before handing it off. Preserve known text fallbacks and
explicit refusals; do not equate refusal with completed exact support. The
[type matrix](type-contract-strategy.md) and [value evidence](value-contracts.md)
carry detailed native-type targets. Do not rebuild completed scalar/BSON/export work.

| Packet | Scope / files to inspect | Deliverable and completion evidence |
| --- | --- | --- |
| B3-P1: reconcile coverage | `docs/type-contract-strategy.md`, `docs/b3-test-scenario-survey.md`, `docs/value-contracts.md`; inspect corresponding tests | Map each remaining type/consumer to exact support, exact text, refusal or untested. Include September 27–28 MongoDB top-level edits and nested numeric/text cases. Name one smallest uncovered case and its runnable command. Documentation only; no broad feature implementation. |
| B3-P2: PostgreSQL boundary | `crates/drivers/postgres/{src,tests}`, affected core literal/parser path only | One calendar/array/native-type gap from P1, with an independent native wire/server oracle and a consumer round trip. Record baseline failure or already-covered result; add no type abstraction without a caller. |
| B3-P3: consumer parity | One format in `crates/core/src/export/`, affected driver test and existing value corpus | One missing empty/NULL, float/subnormal, delimiter/newline, temporal or nested case. Parse output and compare exact value/type; verify destination preservation on refusal. |
| B3-P4: grid edit / bind | One driver plus `crates/app/src/ui/browse_tab/value_parse.rs` and existing edit/parser tests as needed | One mixed-storage/BSON or exact numeric edit gap. Assert original row identity and native persisted value. Ranges beyond safe support must refuse visibly without a lossy write. |
| B3-P5: delivery / SQL boundaries | Existing core planner/lexer/parameter tests or one driver's result tests | One comment/CRLF/Unicode/malformed-tail or row-cap/zero-row/mid-stream case. Assert statement identity/order or result completeness and connection state. |
| B3-P6: mutation triage | Retained quality reports and the tests for one changed decoder/consumer | Classify one survivor/timeout group; add independent assertions for genuine misses and rerun that scope. Missing reports are blocked evidence, not equivalent mutants. |

P2–P5 must also retain the remaining driver targets in the matrix: DuckDB
interval/collections and high-precision bindings, SQL Server temporal/money/variant,
ClickHouse bounds/wide/nested values, MySQL session modes, Redis nested/binary
semantics and MongoDB mixed-type editing. Audit first; do not silently remove
those targets to mark B3 done. Close B3 only when each in-scope contract has
explicit evidence and outstanding correctness findings are resolved.

### B4 next order

Use the board's recorded decisions. Task IDs below belong to B4 lanes, not the
sprint milestone numbers. Keep one owner for shared editor/policy files.

| Order | Tasks | Outcome / dependencies |
| --- | --- | --- |
| 1 | E1 then E2; G3 | Refuse implicit/unterminated shared transactions; refuse cached sessions when key material cannot be verified, including after connect. Separate policy and agentd packets. |
| 2 | F1; F3 then F5 then F2 | Capability-correct Stop; session identity guards; retired toggle; awaited rollback before close/disconnect. One editor owner, sequential commits. D3 is already available. |
| 3 | F4 and F9 | Retire old editor sessions on reconnect and consume the built-in tunnel closed-state API in the monitor. Coordinate with the editor owner. |
| 4 | F6; F8 | Confirm built-in host keys on connect/reconnect; isolate unknown-write blocking/recovery by connection. Include denied and allowed paths. Shared trust/audit changes need review before closure. |
| 5 | C6; G5 | Real MySQL/SQL Server TLS through SSH, and unattended agentd OpenSSH refusal/success. Use existing TLS/SSH fixtures; mocks cannot qualify these tasks. |
| 6 | I2, I3, I5; then F7 | Explicit Flatpak refusal and accurate docs/tier ownership; transport audit records; isolated Session GTK flow after lifecycle fixes. I4 manual steps were added in this review; runtime checks remain pending. |
| After Arch | I1 | Required Debian rules recipe and validator askpass fix for the GNOME/Debian phase. |

A, B, D and H regressions stay retained. Their per-task evidence is archived on
the board. Run affected layers after integration, especially after the September 28
MongoDB/function refactors; earlier lane passes do not verify the final tree.

### Arch / Omarchy / Wayland UI packets

Use [the manual checklist](manual-verification-0.2-features.md) and the existing
[Omarchy package guide](omarchy.md). Work on application widgets and resources;
this plan does not request changes to the user's Hyprland or system configuration.

| Packet | Scope | Acceptance |
| --- | --- | --- |
| UI-A1: connections / SSH | Connection dialog/list, driver defaults, TLS/SSH prompts | Defaults match each driver; editing saved values preserves them; narrow dialog title/actions fit; focus/Enter/Escape work; host-key decline writes nothing. Native Wayland, light/dark screenshots. |
| UI-A2: editor / session | Stop, Session labels/prompts, reconnect, close/disconnect, Open/Save/recovery | Verify B4 F1–F9 applicable paths and B5 file-dialog/dirty-close/restart behavior with database/audit postconditions. No stale session labels or unfinished rollback. |
| UI-A3: grid / export / catalog | Browse shortcuts, full long-cell and typed edits, import/export, catalog | Correct shortcut glyphs/actions; exact persisted values; error/empty/denied states; restricted-role and stale catalog results; light/dark screenshots. |
| UI-A4: installed candidate | Arch package, desktop entry, askpass, GSettings, profile isolation | Freeze a clean SHA; install, upgrade and rollback without losing connections, secrets, drafts, history or audit. Record native Wayland backend, scale, monitor layout, versions and package checksum. |

For A1–A3 check keyboard navigation, clipboard, context menus/popovers, dialog
placement, scrolling, resizing and fractional scaling on the actual Omarchy
session. Treat Xvfb `widgets`/`ui` as automated regressions, not native Wayland
acceptance. Screenshots and checkmarks must identify the binary/package SHA and
actual backend. Record failed, blocked and not-run items explicitly.
B7 retains 30 consecutive retry-free GTK attempts across at least six runs at one
candidate SHA, plus installed Arch Wayland acceptance. If qualification targets
change, record that decision. GNOME/Debian acceptance remains required after Arch.

### Required next phase: GNOME on Debian Wayland

Start when UI-A1–A4 have recorded Arch results. This is sequential work in the
0.2 continuation plan, not a removed or optional target. Use a clean Debian VM
with GNOME/Wayland and system libraries that satisfy the current GTK build.
Record the exact Debian release and library versions rather than assuming that
an arbitrary stable release meets the GNOME 50 API requirements.

| Packet | Work | Completion evidence |
| --- | --- | --- |
| UI-D1: Debian package | Fix B4 I1 in `packaging/debian/rules`; validate askpass, binaries/aliases, desktop/AppStream files, GSettings and resources | Build and inspect a real `.deb` at the recorded SHA; package validators pass and required files are installed |
| UI-D2: GNOME desktop | Repeat UI-A1–A3 and applicable manual checks under native GNOME Wayland | Light/dark screenshots, keyboard/focus, clipboard, dialogs/file chooser, notifications, keyring and scaling; database/audit postconditions at the installed SHA |
| UI-D3: migration and rollback | Install, upgrade, restart and rollback using existing migration procedures | Connections, secrets, preferences, drafts, history and audit remain recoverable; profile isolation holds |
| UI-D4: reconcile qualification | Compare Arch/GNOME results and resolve platform-specific defects | Both environments have explicit pass/fail/blocked results; fixes rerun affected checks on both targets |

B7 closes desktop qualification only after both installed passes and the existing
candidate gates. If the Debian pass changes source, freeze the resulting SHA
and rerun affected Arch checks; the earlier package does not certify the new one.
Publication remains a separate action.

### Immediate execution checkpoints

1. **Inventory:** run B3-P1 against current `linux`; return a type/consumer matrix,
   completed-case evidence and one smallest untested case. Inspect September 28
   refactors first so the task uses current module paths. This is a bounded
   documentation handoff, not another whole-repository audit.
2. **B3 implementation:** take one P2/P3/P4/P5 case at a time. Reproduce, preserve
   exact server/file expectations, fix the conversion boundary, and run the
   affected crate plus selected shared layers. Triage relevant P6 mutation
   findings alongside the changed logic. Update the evidence ledger per commit.
3. **B4 implementation:** begin with E1, E2 and G3; continue through the ordered
   editor, monitor/trust, TLS/daemon and audit tasks above. Give Luna one task ID
   and explicit file ownership. Review shared security/lifecycle changes before
   marking the task complete. Do not rerun completed lanes without an affected
   change or unresolved failure.
4. **Arch UI:** complete A1, A2 and A3, fixing reproduced layout/interaction bugs;
   then freeze and package A4. Render each changed surface in light and dark on
   actual Omarchy Wayland and record database/audit outcomes for safety flows.
5. **GNOME/Debian:** complete D1–D4 after Arch, then reconcile B7 against the final
   candidate. Unchecked installed flows and failed/incomplete mutation measurements
   remain open until resolved with evidence.

For each implementation handoff, request: initial SHA and status, one failing
reproducer, smallest patch, regression command/results, applicable layer reports,
remaining gaps and resulting SHA. No new abstraction, dependency or test runner
is needed just to dispatch these tasks.

### Ready-to-use Luna handoff

Choose **B3-P1** for the first Luna task. Subsequent tasks take one case or one B4
ID from the tables above, with its file scope and dependencies. Luna's returned
changes need review for cross-consumer and security effects before marking done.

```text
Worktree: /home/trung/Projects/tablepro; working branch: linux.
Read CLAUDE.md, PLAN.md, docs/bookie-0.2-sprint.md and docs/validation-playbook.md
(paths under linux/ for docs). Follow the RTK instruction for shell commands.
Confirm current full HEAD and git status; reviewed baseline was
fe22716459c242b350bf34e7070dde19bba184ba. Record any newer commits or local edits.
Task: B3-P1 only. Reconcile remaining type/consumer coverage against current code
and the archived September 26–28 review. Update the three B3 evidence documents.
Allowed edits: linux/docs/type-contract-strategy.md,
linux/docs/b3-test-scenario-survey.md, linux/docs/value-contracts.md.
Return a concrete smallest next case, exact test command, existing evidence SHA,
and open gaps. Do not claim new runtime passes from inspecting test source.
No GNOME/Debian VM setup, production-code edits, release, push or publication.
```

For implementation packets, fill the [validation playbook task template](validation-playbook.md#agent-task-template)
with the selected ID, allowed paths, initial SHA, narrow reproducer, runner layers
and fixture ownership. Run the narrow regression first, then applicable `full`,
`change-contracts`, `values`, `security-policy`, `ssh`, `tls`, `drivers`,
`postgres-release`, `widgets` or `ui` layers from `linux/`. Existing tiers own the
tests; no duplicate runners. Record actual commands, before/after results,
reports, resulting SHA and all skipped/blocked scopes. Documentation packets
need diff/link checks only. Serialize Cargo and Docker on a shared checkout.

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
between this and any release candidate. Current sequencing is B3, B4, then
Arch/Omarchy Wayland acceptance, as recorded in the continuation plan above.

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
- [x] **B2 runtime/storage**: owned Tasks, explicit stores, private durable writes,
  history migrations, GSettings, coalesced writers. Remove replaced globals/runtime
  calls; flush persistence and settle governed operations at shutdown.
- [ ] **B3 lossless contracts**: upstream values, columns/results, errors, typed
  dialect/filter/write, metadata/row identity. PostgreSQL/MySQL/SQLite/SQL Server/
  ClickHouse/Redis/MongoDB/optional DuckDB. Exact values through every consumer.
- [ ] **B4 transport/sessions**: network/OpenSSH/session integration in GUI/daemon,
  policy/audit guarded. Dedicated editor sessions, cancellation, transaction ownership,
  terminal outcomes/retirement. No weaker auth/TLS fallback.
  Open work is split into small tasks on the [B4 task board](b4-task-board.md).
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

## Original schedule, models and verification (historical)

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

## Remaining-work review: 2026-09-26

| Milestone | Implemented evidence | Work still required before closure |
| --- | --- | --- |
| B1 | Host build, prior Meson evidence, package fixtures | Real Flatpak and installed Arch/Debian/profile qualification at the candidate SHA |
| B2 | Owned stores/tasks, migrations, rollback copies, MCP shutdown | Requalify installed upgrade/rollback during B7; do not reopen completed code without a failing case |
| B3 | Typed row edits, metadata fixes, undecodable-write refusal, five-engine binary INSERT export | Remaining wide numeric/array/temporal/JSON consumer contracts, optional DuckDB, and all-engine acceptance. A separate declared-type field remains deferred until a concrete consumer needs it; the collation-only September 25 note does not prove the full lossless checklist above |
| B4 | Governed sessions, OpenSSH/agent authentication, container and logic tests | Complete installed session/SSH prompts, cancellation, cleanup and TLS-through-tunnel acceptance; retain unsupported-engine behavior |
| B5 | Open/Save/Save As, external-change checks, restart relinking | Installed file-dialog/dirty-close/recovery flows and shared-planner highlighting acceptance |
| B6 | Guarded catalog listing shared by GUI/MCP, real PostgreSQL fixtures | Restricted-role/refresh/stale-result and installed Catalog workflow acceptance |
| B7 | Fast, driver and isolated widget evidence available | Freeze a candidate, run missing security/optional/package tiers and retry-free Wayland soak; reconcile release evidence at that SHA |

Continue B3 with one consumer contract at a time, then close B4–B6 acceptance
against the implemented code. The [manual checklist](manual-verification-0.2-features.md)
is still open; isolated widget tests do not check its boxes automatically.

### B3 precision and export review: 2026-09-26

Start SHA: `3f73b8c6ce8a5f8bff4058c26b5ee8d89115094c`.

This packet addresses concrete value changes found during review:

- MySQL decimal decoding rounded long fractions. ClickHouse also passed JSON
  numbers through floating point. Exact parsing now keeps unsupported precision as text.
- Grid edits and typed filters accepted decimals by rounding them. They now
  refuse input outside the exact editable range.
- ClickHouse non-finite floats became NULL in its JSON response. The query now
  requests quoted non-finite values. JSON exports and MCP responses preserve those values as text.
- SQL Server INSERT exports used invalid boolean literals and lost Unicode.
  Numeric booleans and Unicode literals preserve both; column comments retain
  one Unicode prefix.
- DuckDB binary INSERT exports now use the engine's hexadecimal decoder.

Verification: 443 core, ClickHouse and DuckDB unit tests passed, including
DuckDB's NULL/empty/all-byte binary round trip. All 56 ClickHouse, MySQL and
SQL Server Docker integration tests passed. Each new failure was reproduced
before its fix. `ci-local.sh full` passed with report
`target/quality/20260926T140557326075Z-full/report.json`. Debian package fixture
checks were skipped on this host because `dpkg-deb` is absent; packaging is not
qualified by this value-contract packet.

GTK fixture follow-up: hosted run `36245849080` at `3f73b8c6c` passed every
executed job except installed GTK smoke. Its CSV scenario expected a 100-row
page but read the 1,000-row preference default. A private D-Bus reproduction
confirmed that dconf retained the activation environment while scenarios changed
config roots. The fixture now uses a persistent keyfile backend per scenario.
All 19 host Xvfb smoke scenarios passed with the current debug build; hosted
release-build smoke and candidate Wayland qualification remain separate.

These changes do not close B3. PostgreSQL wide NUMERIC decoding now preserves
exact text; arbitrary-precision editing remains open. Optional DuckDB native
temporal/interval/collection values, nested values, and exact values through
MCP and every export format still need dedicated acceptance. Spreadsheet
integer/decimal export now has an exact-value regression and fix; floating-point
and temporal spreadsheet cells still need dedicated coverage. In particular, a
visible undecodable marker is safer than NULL but does not satisfy lossless
support for the remaining unsupported types. Rejecting an inexact decimal edit does not add arbitrary-precision
editing. B4–B6 acceptance follows B3; their boxes remain open.

### B3 regression research: 2026-09-26

The [external scenario survey](b3-test-scenario-survey.md) pins test sources from
DBeaver, Beekeeper Studio, pgcli and SQLFluff. Use their scenarios and issue-linked
regressions to find gaps, reproduce failures locally, and retain permanent tests.
Common PostgreSQL scalar arrays now preserve their elements and dimensions
through SQL export/re-import. Temporal/nested values, unsupported array element
types and remaining consumer parity stay next. Secure transport and authorization scenarios feed B4 acceptance;
the VM remains optional for current B3 work. The survey is an initial sample,
not evidence that those projects or BookiE pass every scenario.

The next B3 checkpoint fixes end-of-day time changing to midnight and adds
timetz offset preservation. Eleven inputs run under three time zones through
server byte comparisons, bindings and SQL exports. The value runner now verifies
that every listed test actually passed; zero-execution success is rejected.
Mutation measurements are required to fail visibly, with reports retained.
Fresh scoped array/time mutation results and remaining temporal cases are in
[value contracts](value-contracts.md#postgresql-time-checkpoint). This does not
close B3 or the later B4–B6 acceptance gates.

The following checkpoint fixes PostgreSQL BC/extended-year SQL export syntax
with permanent unit and real-server round-trip tests. The
[CI evidence audit](ci-audit-2026-09-27.md) also closes eight orphaned SSH tests,
adds explicit required-job status checking and records the broader mutation
findings. PostgreSQL boundary/type assertions now catch those survivors in scoped
local reruns; core's interrupted mutation run remains a triage backlog. B3 remains
open for the remaining temporal/nested values and consumer contracts.

The next consumer checkpoint covers XLSX temporal precision and timezone identity:
fractional times/timestamps, timezone-bearing timestamps and dates outside Excel's
1900–9999 native range are written as exact text. Supported dates and whole-second
times remain numeric cells. Regressions inspect workbook XML rather than only
checking that a ZIP file was produced. Finite/nonfinite float and NULL contracts
also cover two hosted mutation survivors. This is scoped spreadsheet evidence;
remaining driver types, arbitrary-precision editing and B4–B6 stay open.

The workbook follow-up adds three real UI scenarios for typed-value export,
save cancellation and explicit empty-text refusal, plus negative tests of the
workbook XML oracle. It also corrects XLSX coordinate validation to the actual 16,384-column / 1,048,576-row
sheet limits (one row reserved for headers), including overflow-safe diagnostics.
Public export regressions preserve existing files on excessive columns and
mid-write cancellation. Remaining B3 consumers/types and B4–B6 stay open.

The workbook UI corpus also exposed empty strings being silently omitted by the
XLSX library. Export now refuses this lossy conversion with the data row/column
and CSV/JSON alternatives; UI and atomic-file regressions cover that refusal.

The next regression checkpoint broadens the shared contract to mixed, repeated
named bindings through all six SQL drivers. Two installed UI scenarios check
exact committed values and cancellation/retry; parser, transport option/refusal
and export write-failure regressions cover adjacent boundaries. Evidence and
scope are recorded in [value contracts](value-contracts.md). B3 remains open.

The XML consumer checkpoint reproduces carriage-return normalization through an
independent parser and silent replacement of unsupported characters. XML now
preserves CR/CRLF and refuses illegal characters with a row/column diagnostic,
leaving existing destinations intact. Two default UI scenarios and negative
oracle tests cover the round trip and visible refusal; this does not close B3.

### B3 shared boundary contract and build reuse: 2026-09-26

The [value-contract suite](value-contracts.md) uses one corpus across all eight
drivers and the grid, filter, CSV import, named parameter, JSON and MCP paths.
SQL engines exercise binding and literal export separately. Driver fixtures do
not stand in for every server version or every native type.

The new suite reproduced and fixed float overflow/underflow in input parsers,
floating-point named decimal parameters, MySQL float-export underflow, SQL Server
float-literal limits, ClickHouse decimal-export rounding, dropped empty Redis
arguments and missing DuckDB parameterized queries. ClickHouse's explicit SQL-size
limit is retained, and long result values are checked independently.

All 11 selected suites passed twice, including optional DuckDB and the grid
parser. The unchanged run compiled in 0.924 seconds with 745 fresh artifacts and
zero rebuilt packages. Reports and remaining type coverage are linked from the
suite guide. The local integration runner now includes MongoDB and selects the
six server drivers together. No existing build artifacts were deleted.

B3 remains open for the native-type and consumer gaps listed above. B4–B6
acceptance stays next in sequence.

### B3 review and connection form: 2026-09-27

Rechecked the 0.1.5 fixes against the source, regression suites and sprint
ledger. The listed driver fixes remain covered. DuckDB nanosecond and
TIMESTAMPTZ parameters still require explicit casts; MySQL BIT(1) has both an
isolated grid-edit widget test and a driver update round trip, while a complete
installed-app-to-MySQL edit acceptance test remains open. MySQL backslash-bearing
column comments are refused until session-aware DDL exists. ClickHouse values
outside DateTime64 bounds are refused locally; the live-server contract pins the
server's lower-year clamp and upper-range error behavior.

The new-connection dialog now uses a centered header title that follows driver
selection and has a wider content area. The installed-app AT-SPI scenario checks
the full title, help text and minimum available width. The UI and isolated GTK
widget scenarios passed locally; hosted CI and the remaining B3 acceptance gaps
remain separate.

A follow-up value-path audit caught two silent shape-loss cases. Duplicating a
row changed an undecodable cell or missing source value to SQL NULL; the action
now refuses the draft and reports the affected column or shape mismatch. The
ClickHouse response reader zipped column names with types and filled missing row
cells with NULL; it now rejects header and row-width mismatches while preserving
explicit NULL cells. Regressions were observed failing before the fixes. The
ClickHouse library tests (32) and app library tests (384 passed, 5 ignored) pass
afterward. Broader cross-driver consumer parity remains open.

### B3 MongoDB nested BSON preservation: 2026-09-27

A new unit regression reproduced nested BSON type loss: Decimal128, binary and
date values became ordinary debug strings inside `Value::Json`. Nested documents
and arrays now use canonical Extended JSON. Top-level Decimal128 extremes remain
exact text, and BSON dates outside chrono's RFC3339 range use canonical Extended
JSON with the original millisecond count. Generic binary stays editable as bytes;
all other BSON binary subtype tags retain canonical subtype metadata. MongoDB 7
Docker coverage inserts native Decimal128, date, UUID and user-defined binary
fixtures and reads them through the driver query path. All 24 MongoDB unit tests
and all eight real-server integration tests passed. Scoped mutation testing
caught three of four generated mutations, with one unviable whole-function
replacement and no survivors or timeouts. The real-server contract verifies
nested Extended JSON through JSON export, and an MCP unit contract verifies the
value conversion does not flatten it. CSV is parsed back into fields to verify
that quoting preserves the nested value. The real-server scenario also sends
its query result through the public XLSX exporter and checks Decimal128, date
and binary subtype markers in workbook strings. All eight MongoDB integration
tests, including ignored Docker cases, passed. A second MongoDB 7 scenario uses
the keyed-update SQL shape for nested document and array cell edits, then checks
native Decimal128, date, binary subtype and integer values through a direct BSON
client. MongoDB 7 fixtures now also verify native BSON reconstruction from the
driver's canonical Extended JSON insert path and preserve nested markers through
the MCP browse tool. BSON Timestamp, regex and MinKey columns now classify as
JSON and round-trip through live grid edits as their native BSON kinds. Other
special BSON types and mixed-type-column edits remain open; B3 is not closed.

The MongoDB 7 query/export/import scenario now also checks a nested Int64 above
2^53, explicit nested BSON null and Unicode text through driver results, JSON,
CSV, XLSX and Extended JSON re-import. The MCP browse scenario asserts the same
values on a real driver connection. Both targeted Docker-backed regressions
passed on 2026-09-28.

### B3 SQLite dynamic storage classes: 2026-09-27

A file-backed integration scenario checks TEXT, REAL, INTEGER, BLOB and NULL
values in a declared `NUMERIC` column through bound edits. It compares SQLite's
`typeof` result with the decoded TablePro value, then exports rows as SQL INSERT
literals and verifies the same storage classes and values after re-import. All
21 SQLite integration tests passed. Policy-guarded CSV import now also preserves
text and numeric values in INTEGER, REAL and NUMERIC affinity columns. Installed
grid acceptance across affinity transitions remains open.

The policy-guarded SQLite CSV import had a separate failure: strict numeric
parsing rejected legal text stored in a NUMERIC-affinity column. SQLite imports
now bind unparseable INTEGER/REAL/NUMERIC fields as text so SQLite can apply its
own affinity; other drivers remain strict. A real file-backed test verifies
text and decimal rows through batched, audited import for INTEGER, REAL and
NUMERIC columns. The reproducer failed before the fix. Installed grid acceptance
remains open.

### B3 XLSX nested JSON consumer check: 2026-09-27

A core workbook regression verifies that nested canonical Extended JSON keeps
its Decimal128, binary subtype and millisecond-date markers in an exact XLSX
text cell. The focused case and all 436 core library tests passed. A separate
MongoDB 7 test sends a live query result through the public XLSX writer and
checks BSON markers in the workbook; all eight integration tests passed.

## Documentation and boundaries

Link this sprint from PLAN.md/ROADMAP.md and mark the 0.1.1 plan superseded while
retaining history. Keep shared Rust close to upstream, import with attribution,
isolate branding/governance/compatibility/additional drivers. Never merge Apple
source. Prepare upstream-compatible fixes separately. Contacting authors, upstream
PRs, publication and repository rename are outside this task's authorization.

Status on 2026-09-25: A1–A4 are implemented and ticked; their evidence is from
the 0.1.2–0.1.4 work, not from a frozen 0.2 candidate. A5, B1 and B3–B7 stay open.
B1 lacks a real Flatpak build and installed-package checks. B2 now includes
private pre-migration backups for history, preferences and window geometry, a
GSettings schema shipped by Meson/Arch/Flatpak, and JSON mirrors for package
rollback. B5 has Open, Save, Save As, a
changed-on-disk check before every save and a close prompt, and a restart
relinks each tab to its file (an absolute, bounded path added to the workspace
record; older builds ignore the field). B6 lists views and materialized views, and a guarded
`list_objects(kind, schema)` shared by the Catalog window and the MCP tool covers
routines, triggers, sequences, extensions, roles, enum/composite/domain/range
types and per-table privileges by role. Types are a kind of `list_objects` rather than a
separate `list_types`. B3 is narrowed for 0.2 to what the single type string
loses in practice: a column collation, read on PostgreSQL and MySQL and kept by the
structure editor's alter statements. Schema columns keep the declared spelling in
`data_type`; a separate declared-type field waits for a consumer. SQL Server
collation is now read from the catalog and restated on ALTER COLUMN, with a
real-server regression check. B4: agentd refuses unknown SSH host keys; lone
transaction statements are refused
on shared connections; PostgreSQL, MySQL, SQL Server and SQLite-file editor tabs can opt into a
governed dedicated session (in-memory SQLite, ClickHouse and the other engines
answer that sessions are unsupported; an interrupted SQL Server session is retired,
not reused). A saved connection
can use the system OpenSSH client (forced host-key checking, per-host secret
binding, ProxyJump from ssh_config); the GUI prompts through GTK and agentd runs
unattended. The askpass helper ships in the Arch, Debian and Meson builds; Flatpak
cannot reach the host ssh. B4 decision 2 requires explicit refusal with a
clear message; implementation and both documentation paths remain task I2.

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
  the file-size guard passing. B2's "remove replaced globals" goal was
  completed in this pass. GSettings and history migration were completed in
  the 2026-09-25 checkpoint below.

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

- 2026-09-25 working tree, B4 safety follow-up: a failed COMMIT or ROLLBACK
  retains its governed transaction batch until a confirmed ending. A failed
  editor Commit now leaves the dedicated session open, reports the error and
  permits retry; closing it still follows guarded session cleanup. Editor Save
  and Close keeps the tab open when text changes after the save snapshot, and a
  closed connection cancels a pending Catalog listing. Focused tests passed: policy 151, app 356
  (3 ignored), PostgreSQL session container 6, MySQL session container 1,
  OpenSSH container 7; app/policy Clippy passed with warnings denied. These
  checks do not verify the new GTK paths on an installed package; the 0.2
  manual checklist and candidate gates remain open.

- 2026-09-25 working tree, B3 SQL Server collation: `fetch_columns` reads
  `sys.columns.collation_name`, and column DDL restates the validated collation
  during ALTER COLUMN. Six focused builder tests and one SQL Server container
  round trip passed; the full core unit suite passed (395 tests). B3 remains
  open for its other value and metadata contracts.

- 2026-09-25 B2 completion checkpoint: preferences and window geometry migrate
  into isolated stable/development GSettings schemas. The original JSON files
  receive private backups before migration and remain current for package
  rollback; newer builds reimport changes made by an older package. History
  schema upgrades snapshot the SQLite database with `VACUUM INTO` under the
  storage lock before applying transactional migrations. Focused migration and
  rollback tests pass. The development Meson build/install compiled both
  schemas and installed the GUI, daemon and SSH askpass helper. The real-driver
  integration gate passed: PostgreSQL 21, Unix socket 2, MySQL 14, SQL Server
  16, ClickHouse 14, Redis 1. A5 still needs an immutable candidate and soak;
  B1 still needs a real Flatpak build and installed package qualification.

- 2026-09-26 audit review: reviewed and merged an independent audit pass over
  the column-default fix, the activity-view byte label, the SSH tunnel `relay`
  refactor, and new duplicate-row/keyed-edit tests. Two problems surfaced in
  review rather than in the audit itself:
  - MariaDB 10.2.7+ already reports a column default as the SQL that defines
    it, and reports a column with no default as the text `NULL` rather than
    SQL NULL. The MySQL driver's new default handling quoted that text again,
    so a MariaDB column with no default would have gained the literal default
    `'NULL'`. Caught against a real MariaDB 11 container before merge; fixed
    with a server-flavour flag read from `VERSION()` and a regression test
    that fails without it.
  - The already-pushed connect-dialog timeout commit left helper functions
    after the `#[cfg(test)]` module, which fails Clippy's `items_after_test_module`
    lint on the app crate. `preflight.sh` does not lint the app crate, so this
    had not been caught. Fixed by moving the tests back to the end of the file.
  Full verification on the combined tree: preflight, app Clippy and lib tests
  (369 passed), and the PostgreSQL (23), MySQL (17), SQL Server (19, full
  suite), and SQLite (16) driver container tests all passed. `cargo deny`, the
  GTK tiers, ClickHouse/MongoDB, and the release tier were not run.
- 2026-09-26 long-text/JSON cell edit: reviewing the merged audit for what it
  missed (rather than just what it added) surfaced a separate, older bug: a
  grid cell holding text or JSON over 40,000 bytes truncates for display and
  appends `"… (+N more chars)"`. Double-clicking that cell to edit it seeded
  the edit widget from that truncated label, not the real value, so any typed
  change (or the JSON popover's Save) could persist truncated text, and the
  literal `"… (+N more chars)"` marker if it wasn't deleted first. Opening and
  closing an edit with no change was already safe. Fixed by carrying each
  cell's full, untruncated text in a widget-data slot set at bind time and
  reading it back wherever an edit starts, seeds the JSON popover, or checks
  for a no-op commit, instead of reading the label. A regression test lives in
  the grid's isolated-display (`gtk`) tier, since it must render a real
  `ColumnView` bind cycle; this environment has no `xvfb-run`, so the test is
  confirmed to compile but not yet confirmed to fail pre-fix and pass
  post-fix on a real display. See
  [manual-verification-0.2-features.md](manual-verification-0.2-features.md)
  for the manual check.
  Not covered by this fix, tracked separately: `sql_literal.rs` renders any
  `Value::Bytes` as `/* bytes omitted */ NULL` in SQL export and copied
  INSERT statements, which is a deliberate, marked, but real loss of binary
  column data. No test or fix attempted yet.

- 2026-09-26 local/remote reconciliation: remote `8ae99ad66` is the integration
  base; original local B2/B3 work is preserved at `47c4dd5d4`. Retained remote
  sessions, SSH, catalog, defaults, long-value edits, history snapshots and window
  migration. Combined preferences safeguards with the remote schema IDs, paths,
  rollback import and packaging, removing the duplicate local schema/installers.
  Retained local MCP graceful shutdown, CSV header boundary fix, history
  regressions, invalid-UTF-8 byte preservation, and rejection of undecodable
  parameters. SQL INSERT exports now reject binary/undecodable/non-finite values
  instead of substituting NULL; this addresses the export-loss item above by
  refusing the lossy operation, not by implementing binary SQL literals. B3 is
  still open. See [reconciliation-2026-09-26.md](reconciliation-2026-09-26.md)
  for decisions and combined-tree verification.

- 2026-09-26 continuation from `41fd9876d`: pushed the reconciliation to
  `fork/linux`. Hosted preflight exposed a Debian fixture missing the newly
  required schema; `541074adb` adds the schema and a missing-schema rejection
  case. The fixture passed in a disposable Python/Debian container with the
  repository mounted read-only. Continued B3 with a fallible SQL literal API:
  binary INSERT exports preserve exact bytes for PostgreSQL/MySQL/SQLite/SQL
  Server/ClickHouse; undecodable/non-finite/unsupported values return errors.
  The SQLite regression failed with binary export disabled, then all five
  real-engine round trips passed for NULL, empty binary and all 256 byte values.
  Core library tests passed (405). DuckDB binary export remains explicitly
  unsupported. Copy as IN keeps its existing skip behavior. Full fast checks passed at
  `target/quality/20260926T133550496651Z-full/report.json`. Hosted CI remains
  a separate gate for the resulting pushed commit.

### B3 DuckDB native temporal checkpoint — September 27

DuckDB date/time/timestamp and enum results no longer expose implementation debug strings. Native regression cases prove pre-epoch nanoseconds, timezone instants, infinities, end-of-day time, BC/large-year dates, SQL/parameter round trips and JSON precision. Unsupported intervals, collections and calendar ranges are visibly undecodable with write refusal coverage. Full nested/interval decoding, other driver-native gaps and consumer parity remain open; B3 is not complete. Reproduction and validation details are in [value contracts](value-contracts.md#duckdb-native-temporal-and-enum-checkpoint).

### B3 PostgreSQL interval and temporal-array checkpoint — September 27

PostgreSQL interval tests now compare independent native fields across 59 deterministic boundary/generated cases and all four IntervalStyle settings. Extreme-hour exports and ambiguous mixed signs are fixed. Date/timestamp infinities and date/time/timetz/timestamp/timestamptz/interval array elements now preserve their values through native wire comparisons, bound parameters, SQL INSERT and JSON checks, including non-UTC session imports. Malformed element payloads and unsupported calendar ranges remain explicit refusals. See the [type-contract strategy](type-contract-strategy.md) for remaining per-driver targets; B3 remains open and Oracle stays deferred.

### B3 MySQL native temporal checkpoint — September 27

Zero dates no longer read as NULL, negative TIME values keep their sign, and extended TIME, dates with zero parts and YEAR values are exact instead of undecodable. They survive parameter and SQL export round trips, checked by the server. A SQLite regression from automatic decimal parameters, which were bound as text, is also fixed. Details are in [value contracts](value-contracts.md#mysql-native-time-zero-date-and-year-checkpoint). B3 remains open.

The MySQL `TIMESTAMP` session-zone contract is now explicit: pooled connections reset to UTC on checkout, while a dedicated session with a non-UTC zone returns non-NULL `TIMESTAMP` values as undecodable. A Docker regression checks the session-local text and independent epoch value, then verifies the pool reset and the existing UTC temporal round trip. See the [value contract](value-contracts.md#mysql-native-time-zero-date-and-year-checkpoint). Broader SQL-mode, UI and consumer coverage remain open; B3 is not complete.

### B3 audit of the September 26–27 commits — September 27

Twenty B3 commits were audited. For each fix, its production change was reverted to confirm that its regression test fails, and sibling paths were checked for the same defect. Every audited fix held. Nine further defects were each reproduced by a failing test first, then fixed:

- SQLite and DuckDB bound automatic decimal parameters as text. SQLite then compared them as text, and DuckDB rounded them.
- PostgreSQL `int2vector` and `oidvector` values were exported as array text that cannot be imported. They are undecodable again.
- HTML exports lost carriage returns and silently dropped NUL characters. CSV wrote empty text the same as NULL.
- SQL Server rejected exported `datetime` literals with fractional seconds, decoded `datetimeoffset` values at the wrong instant, and garbled the sign and digits of wide decimals.

The PostgreSQL interval checkpoint left an older integration test expecting infinities to be undecodable, so the PostgreSQL suite did not pass at `093f68968`. The expectation now uses values that are still undecodable.

Open findings from the audit:
- DuckDB temporal parameters are bound as text, so date arithmetic on a parameter fails with an error. No value is lost.
- ClickHouse drops trailing decimal zeros, and the shared contract cannot see scale.
- ClickHouse inline parameter binding truncates fractional seconds.
- MySQL literals assume backslash escapes.
- SQL Server `datetimeoffset` keeps the instant but not the original offset.
- PostgreSQL `int2vector` could be rendered exactly as space-separated text.

B3 remains open.

### PostgreSQL server disconnect classification — September 29

A failing-first PostgreSQL 16 Docker test terminated an active query backend.
Before the fix, SQLSTATE `57P01` surfaced as an ordinary query error. The driver
now classifies server termination states `57P01`–`57P04` as `Disconnected`, while
preserving `57014` cancellation as a query error. A second failing-first test
found unexpected SQLx socket EOF/reset errors were also reported as `Internal`;
these now map to `Disconnected`, while connection refusal and TLS errors remain
distinct. The Docker contract verifies the terminated query cannot succeed or
hang, and that the pool can answer `SELECT 1` afterward.

All 35 PostgreSQL unit tests and the focused Docker test passed. The SQLSTATE
mutation run caught both generated changes; the socket/error-classification run
caught 7 of 8 mutants, with one unviable. The clean strict runner passed 131
selected contracts across 11 suites with no missing suites at source
`12e795cec416fca9da92ed1c95ae6bd9b77e754e`. Evidence:
`target/quality/20260929-pg-disconnect-mutants-home/mutants.out/outcomes.json`,
`target/quality/20260929-pg-io-disconnect-mutants-final/mutants.out/outcomes.json`,
and `target/quality/20260929T212538645091Z-values/report.json`.

B3 remains open.

### MySQL server disconnect classification — September 29

A failing-first MySQL Docker test killed the connection serving an active
`SLEEP` query. The SQLx unexpected-EOF error surfaced as `Internal`; the driver
now reports SQLx I/O loss as `Disconnected`, while preserving the distinct
`ConnectionRefused` and TLS outcomes. The contract verifies the killed query
fails promptly and the pool recovers for a fresh `SELECT 1`.

The focused Docker case and all 12 MySQL library tests passed. The first scoped
mutation pass exposed a missing refusal-path assertion; after adding it, all
three viable mapping mutations were caught and one was unviable. The clean strict
runner passed 131 selected contracts across 11 suites with no missing suites at
source `064b4947d07d4fddbc2a210c0d658f62cef59805`. Evidence:
`target/quality/20260929-mysql-disconnect-mutants-final/mutants.out/outcomes.json`
and `target/quality/20260929T211702925065Z-values/report.json`.

B3 remains open.

### MongoDB mid-cursor disconnect completeness — September 30

A Docker integration test enables MongoDB's `failCommand` test failpoint in an
isolated fixture, seeds 150 rows, and closes the connection on one `getMore`.
The driver's schema sample completes first; the query then receives its initial
cursor batch and must fail as `Disconnected` when fetching the next batch. The
public query API cannot return rows alongside an error, so this also guards
against treating a partial cursor as a successful result. The focused ignored
integration test passed locally. A separate RESP socket fixture returns one key
from `SCAN`, then drops the browse connection on `TYPE`; the page must fail as
`Disconnected` without returning an incomplete row set. A second browse through
the same driver object must reconnect and return the exact complete row. The
focused test passed. These contracts close mid-cursor/page interruption and
subsequent-operation recovery coverage; B3's unrelated native-type, consumer
and acceptance gaps remain open.

B3 remains open.

### MongoDB UUID binary grid edit — September 30

The MongoDB coverage audit found that the matrix claimed a UUID subtype `04`
grid edit, but the server-backed edit was absent. The existing special-value
keyed-update contract now edits a top-level UUID binary field and verifies the
canonical Extended JSON result plus native `BinarySubtype::Uuid` and all 16
bytes through an independent client. The focused test and complete MongoDB
Docker integration suite passed (24 tests). The matrix now marks UUID subtype
editing as covered; collection-wide heterogeneity and remaining unnamed BSON
kinds are still open.

B3 remains open.

### MongoDB BSON binary subtype contracts — September 30

The follow-up audit extended native-server grid editing across BSON binary
subtypes. MongoDB 7 confirms exact subtype and edited-byte preservation for
Generic, Function, BinaryOld, UUIDOld, UUID, MD5, Encrypted, Sensitive, Vector,
Reserved `0a`, and UserDefined `80`. MongoDB rejects arbitrary bytes for Column
subtype `07` with `NonConformantBSON` code 378, so valid BSONColumn encoding and
a positive edit contract remain open. The focused edit test passed. This is a
native format constraint; do not count the rejection of malformed opaque bytes
as successful BSONColumn editing.

B3 remains open.

### MongoDB BSON DateTime grid-edit precision — September 29

A failing-first app parser test showed that MongoDB `date` cells display RFC3339
instants while the generic parser treated their metadata as date-only. The
driver-aware parser now preserves UTC milliseconds and offsets, accepts extra
fractional digits only when they are zero, and refuses sub-millisecond edits
before update construction. A MongoDB 7 app contract verifies the parser-to-keyed-
update path against the stored BSON millisecond epoch and confirms refused edits
leave the original value unchanged.

The focused parser and server-backed app tests passed. Scoped mutation testing
caught 7 of 8 generated changes; one whole-function default-return mutant was
unviable at compile time, with no missed mutants or timeouts. The clean strict
value runner passed 131 selected contracts across 11 suites with no missing
suites at `93d60ea818fe7d4c283bb86440deb7cf2501fe29`. Evidence:
`target/quality/20260929-mongodb-date-parser-mutants-home/mutants.out/outcomes.json`
and `target/quality/20260929T204726972421Z-values/report.json`.

B3 remains open.

### Redis Pub/Sub and MONITOR stream boundary — September 29

A Redis 7.4 regression first showed `SUBSCRIBE` returning its push
acknowledgement as an ordinary one-shot query result. The same fixture showed
`MONITOR` returning `OK` before entering its event stream.
`CLIENT TRACKING ON BCAST` also returned `OK` while enabling invalidation pushes.
The driver visibly refuses all six subscribe/unsubscribe forms, `MONITOR`, and
`CLIENT TRACKING ON` with trailing options. The live Docker test verifies the
unsupported errors and confirms `PING`, `CLIENT TRACKING OFF`, `PUBLISH`,
`PUBSUB CHANNELS` and `CLIENT LIST` still work on the same connection. The
tracking refusal cases cover BCAST, OPTIN, REDIRECT, NOLOOP and mixed-case
spellings. Scoped
mutation testing first caught 2 mutations, with 2 misses and one unviable
whole-query replacement. After adding live negative assertions, iteration
caught both survivors: cumulatively 4 caught, 1 unviable, no survivors or
timeouts. Evidence:
`target/quality/20260929-redis-async-push-mutants/mutants.out.old/outcomes.json`
and `target/quality/20260929-redis-async-push-mutants/mutants.out/outcomes.json`.

Consuming asynchronous push frames remains open.

The clean combined runner passed after the tracking-push addition at
`af46f18461e98a5ec83833e9c18556211b7f1b56`: 125 selected contracts across 11
suites, with no missing suites. Evidence:
`target/quality/20260929T191639404194Z-values/report.json` (`dirty: false`).

An unignored local TCP test now exercises the full RESP3 attribute frame through
the redis client socket after `HELLO 3`. The payload and attached TTL metadata
must survive as tagged JSON; the strict runner includes it under the
`value_contract` filter. This verifies wire decoding without claiming delivery
of asynchronous server pushes. The clean strict run passed 128 selected
contracts across all 11 suites at `887a1bd61060e5c604e9d4e52cb821de4dcf2202`,
with both Redis stream refusal and attribute wire tests visibly passed. Evidence:
`target/quality/20260929T200632950370Z-values/report.json` (`dirty: false`).

B3 remains open.

### B3 MySQL BIT parser and keyed grid path — September 29

The app now has an unignored parser contract for BIT(1), (2), (8), (63) and
(64), including each width boundary, negative input and the signed-value limit.
A Docker-backed app test takes BIT(1), BIT(8) and BIT(63) edits through the
parser, keyed-update builder and MySQL driver, then checks decoded values and
independent `HEX()` server output. BIT(64) with its high bit set remains exact
bytes and cannot be edited as a signed integer; GTK tests keep wide binary and
spatial values read-only.

The first scoped mutation attempt discovered there was no unignored BIT parser
test; its filter had selected only the ignored Docker case. After adding the
parser contract, all 11 remaining mutants were caught with no survivors or
timeouts. The Docker and parser tests passed. Installed GTK-to-MySQL acceptance,
spatial edits and broader mode coverage remain open.

The strict all-driver runner passed on `c1b393f8d`: all 11 suites and 123
selected contracts passed with GTK and DuckDB enabled and no missing suites.
Evidence: `target/quality/20260929T164906704313Z-values/report.json`.

B3 remains open.

### SQL Server money NULL distinction — September 29

The existing `money`/`smallmoney` contract already proved that non-NULL values
must be refused because Tiberius decodes them through binary floats. The live
fixture now includes NULL rows for both types and confirms those cells remain
`Value::Null`, distinct from the non-NULL `Undecodable` markers. The focused
SQL Server Docker test passed; the ignored-test inventory was regenerated and
validated. The strict combined value runner then passed all 121 selected
contracts across 11 suites, including GTK, DuckDB and the updated SQL Server
fixture; evidence is `target/quality/20260929T162725389033Z-values/report.json`.
Exact money decoding/editing remains open.

B3 remains open.

### B3 PostgreSQL zero-row results and numeric parser mutation follow-up — September 29

The all-driver value runner passed on `0b1dab8b8`: all 11 expected suites were
present and passed, including GTK and DuckDB, with 120 selected contracts and
no missing suites. The report is
`target/quality/20260929T161811049211Z-values/report.json`. The PostgreSQL
Docker suite includes a new result-delivery contract. It first reproduced that
a zero-row `SELECT` returned no column names or types; the driver now recovers
statement metadata for plain and parameterized queries. The test also checks
duplicate aliases and row order.

Mutation follow-up for PostgreSQL wide NUMERIC parsing added integer-only,
fraction-only and exponent values that exceed `rust_decimal`, closing eight of
nine prior survivors. The one remaining survivor changed into a timeout when
the expanded test exercised its non-advancing exponent scanner. The final
in-place run had 8 caught, 3 scanner-stall timeouts and no survivors or
unviable mutants. The timeout bound prevented a hang. The parser regression,
report paths and exact follow-up command are in [value contracts](value-contracts.md).

B3 remains open for the remaining driver/type and consumer targets.

### B3 driver gaps and version 0.1.5 — September 27

Each open finding from the audit was reproduced with a failing test first and then fixed:

- SQL Server `datetimeoffset` keeps its original offset as exact text.
- MySQL text exports read the same with or without `NO_BACKSLASH_ESCAPES`, and BIT and spatial values decode exactly.
- ClickHouse keeps decimal scale and fractional seconds in parameters and SQL exports.
- ClickHouse `DateTime64` results with a named timezone now decode to the correct UTC instant using IANA timezone rules. Ambiguous or nonexistent local clock values are refused as undecodable because the response omits the offset needed to identify an instant. Scales 0, 3, 6 and 9 now have a live fixture round trip.
- ClickHouse refuses out-of-range `DateTime64(9)` parameters and SQL exports before sending them. On the local ClickHouse 24.8 fixture, the same raw SQL literal returns a `DECIMAL_OVERFLOW` error.
- DuckDB binds dates, times and microsecond timestamps natively.
- PostgreSQL `int2vector` and `oidvector` values keep their space-separated form.

The shared contract now requires decimal scale to match.

Grid editing now parses MySQL `BIT(1)` as a boolean and `BIT(2..64)` as bounded nonnegative integers. Values above the shared signed integer range decode as bytes and remain read-only; spatial columns are also read-only because the shared value is their stored binary representation. Parser and grid eligibility tests cover these limits. End-to-end GTK editing against MySQL remains to be accepted in the UI layer.

The source version is 0.1.5 in Cargo, Meson, Arch, Debian and AppStream. Meson had declared 0.2.0, so Meson builds reported a different version than packaged builds. No 0.1.5 package or tag has been published.

Open:
- DuckDB nanosecond and TIMESTAMPTZ parameters bind as exact text, so expressions on them still need a cast.
- MySQL column comments containing backslashes are refused with an explicit error because generated DDL cannot preserve them across SQL modes. A live MySQL regression now creates the same comment under default mode and `NO_BACKSLASH_ESCAPES` on dedicated sessions; it proves the server stores different bytes (one slash versus two). Session-aware DDL execution is still needed to support those comments safely. Run `cargo test --locked -p tablepro-driver-mysql --test integration mysql_column_comment_backslash_literals_depend_on_sql_mode -- --include-ignored --exact --test-threads=1` with Docker available.
- ClickHouse DateTime values in DST folds or gaps, or with an unrecognized timezone, remain explicit undecodable results until the wire format carries enough information to identify the instant.

B3 remains open.

### PostgreSQL built-in multirange coverage — September 29

The existing `int4multirange` metadata blocker now has sibling regressions for
`int8multirange`, `nummultirange`, `datemultirange`, `tsmultirange` and
`tstzmultirange`. Each test case first asks PostgreSQL for independent type,
canonical text, hull and component-count oracles, then asserts that a direct
projection fails with SQLx's unsupported `typtype` metadata error. SQL NULL is
checked separately. A wrong expected `tsmultirange` canonical string was
caught by the initial run and corrected against the real PostgreSQL 16 result.
All six built-in multirange families now have explicit coverage; direct decoding
remains open because SQLx fails before a value reaches BookiE.

B3 remains open.

### DuckDB mixed interval component preservation — September 29

The DuckDB binding exposes interval values as separate month, day and
nanosecond fields. The driver previously discarded all three as undecodable.
It now preserves microsecond-aligned values as explicit text, retaining each
component independently; values with sub-microsecond carrier precision remain
undecodable. The local engine regression compares `typeof`, DuckDB's rendered
value and independent `date_part` month/day/microsecond results, then round-trips
through a generated SQL literal and an explicitly cast bound parameter. The
DuckDB column reader now marks all columns in simple or composite primary keys
from native constraint metadata. The app's grid parser and keyed-update builder
save edited interval text back into a native INTERVAL column; the server
confirms its type and all three components.
This closes the supported mixed-interval read/export/import/keyed-edit path, not
installed GTK acceptance, other interval boundaries or collection support.

Scoped cargo-mutants on `duck_value_ref_to_value` caught 11 of 12 mutants, had
one unviable whole-function replacement, and left no survivors or timeouts. The
first pass exposed a surviving unsigned-BIGINT threshold mutation; explicit
`i64::MAX` and `i64::MAX + 1` decoder assertions were added and the final run
caught it. Evidence: `target/quality/20260929-duckdb-interval-mutants-final/mutants.out/outcomes.json`.

The driver contracts `fetch_columns_reports_composite_primary_key_columns` and `value_contract_interval_grid_edit_persists_native_components` run in the DuckDB integration suite. The app parser/keyed-edit contract is `cargo test --locked -p tablepro-app --features duckdb --lib value_contract_duckdb_interval_grid_edit_preserves_native_components -- --test-threads=1`.

Scoped cargo-mutants on `fetch_columns` caught its empty-result mutation; one
whole-function `Default::default()` replacement was unviable, with no survivors
or timeouts. Evidence: `target/quality/20260929-duckdb-pk-mutants-final/mutants.out/outcomes.json`.

A new MAP refusal contract provides DuckDB `typeof` and exact `VARCHAR` oracles
for a map containing a UHUGEINT above `u64::MAX` and an explicit NULL. The
driver returns an undecodable value, and both SQL-literal and parameter
consumers refuse it. Run
`cargo test --locked -p tablepro-driver-duckdb --test integration value_contract_wide_map_refuses_lossy_consumers_with_native_oracle -- --exact --test-threads=1`.

LIST, fixed ARRAY, STRUCT and UNION each now have native `typeof` and exact JSON
rendering oracles; the driver returns an undecodable value and both SQL-literal
and parameter consumers refuse it. The DuckDB crate suite passes with 35 tests.

B3 remains open.

### ClickHouse `DateTime64(9)` bounds — September 29

The focused ClickHouse 24.8 contract now proves both legal nanosecond endpoints
against exact server epochs. It found asymmetric behavior outside the range:
the lower neighbor is accepted with its year clamped to 1900 while its clock and
fraction remain, and the upper neighbor returns a query error. BookiE continues
to refuse either out-of-range value through bound-parameter and SQL-literal
paths. Raw user SQL can still invoke the server's lower clamp, and the test
records the transformed result so it is not represented as lossless input
handling.

B3 remains open.

### SQL Server `sql_variant` safe refusal — September 29

A failing-first Docker regression reproduced Tiberius's `SSVariant` metadata
panic using a `bigint` value beyond binary64's exact range. A separate server
query verifies the native base type and exact decimal text before the direct
projection is attempted. The driver now catches only this known pinned-library
panic, returns an explicit unsupported error, and retires the affected shared
connection or isolated session. The test verifies both paths and that the shared
connection cannot be reused. This prevents a client panic but does not add
`sql_variant` decoding or editing; exact support remains open.

Scoped mutation testing of the guard first found one surviving “always
unsupported” classifier mutation. Negative assertions for unrelated errors were
added; the final run caught 7 mutations, had 1 unviable mutation, and had no
survivors or timeouts.

B3 remains open.

### PostgreSQL geometric native-type boundary — September 29

The PostgreSQL 16 contract covers the seven built-in geometric types: point,
line, lseg, box, path, polygon and circle. Each is compared with the server's
`pg_typeof` and `::text` oracle, direct projection is an explicit
`Undecodable` marker, and literal/parameter consumers refuse the marker. SQL
NULL remains distinct for every type. The focused Docker regression passed;
this records safe refusal, not geometry editing or shared exact-type support.

The strict combined value-contract runner passed at the same revision: 124
selected contracts across 11 suites, no missing suites. Evidence:
`target/quality/20260929T165940977032Z-values/report.json`.

B3 remains open.

### MySQL spatial grid refusal — September 29

An isolated GTK widget test binds geometry, point and multipolygon byte values
and verifies they render as read-only labels, create no editable cell widgets,
and emit no pending edit. The `widgets` layer passed with the new case included
in the isolated-test registry at clean commit
`2db9f59dc0b1be614440e941baa889f4036bcfb2`; report:
`target/quality/20260929T190108119267Z-layers/report.json`. The focused mutation run caught both generated
changes to the bytes-specific editability guard. A broader 11-mutant function
audit caught 6; its 5 survivors only altered stricter read-only behavior or
unrelated primary-key/generated/auto-increment/mixed guards. Reports:
`target/quality/20260929T184259972000Z-layers/report.json`,
`target/quality/20260929-mysql-spatial-ui-guard-mutants/mutants.out/outcomes.json`,
and `target/quality/20260929-mysql-spatial-ui-mutants/mutants.out/outcomes.json`.

Installed-app acceptance and server-backed spatial grid interaction remain
open; B3 remains open.

### DuckDB TIMESTAMPTZ edit precision — September 29

A failing-first parser test showed that DuckDB grid edits accepted
nanosecond `TimestampTz` input even though native TIMESTAMPTZ casts keep only
microseconds. An embedded server oracle confirms the cast changes the epoch.
The parser now refuses off-microsecond-aligned TIMESTAMPTZ edits before the
keyed update; microsecond-aligned values, including nine-digit forms with
trailing zeroes, remain editable. The app-level test verifies the rejected
edit leaves the existing row untouched and a valid `+05:30` edit stores the
expected UTC epoch. Scoped mutation testing caught 9 of 10 generated guard
mutations, with one unviable whole-function replacement and no survivors or
timeouts. Evidence:
`target/quality/20260929-duckdb-timestamptz-guard-mutants-final/mutants.out/outcomes.json`.

B3 remains open.

### MongoDB top-level ObjectId grid edit — September 29

A Docker-backed MongoDB 7 contract edits a non-key ObjectId field through the
keyed-update path. It checks the returned `$oid`, confirms the row `_id` is
unchanged, and verifies both fields as native `Bson::ObjectId` values through an
independent client. The existing app parser unit covers `$oid` classification;
this closes the server-backed driver/edit gap without claiming collection-wide
heterogeneity or every top-level BSON edit is complete. The focused test passed,
and the clean strict runner passed 129 selected contracts across 11 suites at
`63d67fc915e95c87ffbd8c7f781b60ca56a65657`, with no missing suites. Evidence:
`target/quality/20260929T202504448591Z-values/report.json` (`dirty: false`;
MongoDB selected 8 contracts, including the new edit).

Focused command:

```sh
rtk cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mongodb --test integration value_contracts::value_contract_object_id_grid_edit_preserves_native_bson -- --include-ignored --exact --test-threads=1
```

B3 remains open.

### B3-P6 MySQL spatial editability mutant triage — September 29

The earlier spatial-only GTK mutation pass missed five changes that made more
columns read-only, including an unconditional `false` result. This was a coverage
gap, not evidence that stricter behavior was acceptable: the case asserted that
spatial bytes cannot be edited but lacked a positive editable-column control. An
unignored contract now asserts that a normal VARCHAR column remains editable;
the existing unit contract separately asserts that spatial bytes remain
read-only.

The focused text test passed. A scoped run against the full app library suite
caught all 11 generated editability mutations, with no misses, timeouts or
unviable changes. The clean strict runner passed 131 selected contracts across
11 suites with no missing suites at source
`70206f6fb9a5ce9901ffe4552743e63730100fe0`. Evidence:
`target/quality/20260929-grid-editability-positive-control-mutants-final/mutants.out/outcomes.json`
and `target/quality/20260929T214023821465Z-values/report.json`.

B3 remains open.

### MongoDB wide Decimal128 grid edit — September 30

A failing-first app parser test showed that a valid 34-digit Decimal128 integer
was refused as `Invalid decimal` because `rust_decimal` has a narrower range.
The MongoDB-only parser now validates values beyond that range with BSON's
Decimal128 parser and returns canonical `$numberDecimal` JSON markers so the
driver reconstructs the native value. Invalid syntax and precision beyond
Decimal128 are refused; other drivers retain their existing decimal parser.

A MongoDB 7 app integration test seeds `9.9900`, routes a 34-digit edit through
the parser and keyed-update builder, then verifies the row identity and exact
native BSON value through an independent client. App and driver parser tests
cover a 34-digit fraction, scale preservation, NaN/infinities and refusal of
over-precision input. The focused parser and Docker tests passed. Scoped mutation
runs caught 4 of 5 app parser mutations (one default-return mutant was
unviable), and both BSON Decimal128 helper mutations. The clean strict runner
passed 133 selected contracts across 11 suites with no missing suites at source
`35d457fa488768a5204a26d785c90114073d4acd`. Evidence:
`target/quality/20260929-mongodb-decimal-parser-mutants/mutants.out/outcomes.json`,
`target/quality/20260929-mongodb-decimal-driver-mutants/mutants.out/outcomes.json`,
and `target/quality/20260929T215946474345Z-values/report.json`.

B3 remains open.

### ClickHouse exact lower-scale DateTime64 binding — September 30

A live ClickHouse 24.8 query showed `DateTime64(0)` accepts the exact endpoint
`2299-12-31 23:59:59`, while the parameter and SQL-literal helpers always chose
scale 9 and refused it at the Int64 nanosecond ceiling. The shared temporal
renderer now picks the coarsest scale that preserves every fractional digit and
fits the native calendar and Int64 tick range. Native tests prove year-2299
round trips at scales 0 through 7 through bound parameters and SQL literals,
with value and millisecond epoch oracles. Unit boundaries cover scale 8, the
scale-9 upper endpoint and below-1900 refusal. The first full
suite run caught an obsolete expectation that all values beyond the scale-9
ceiling must refuse; the corrected test distinguishes explicit scale-9 failure
from exact scale-0 support. All 33 ClickHouse Docker integration tests passed
on the final source, as did the core and driver unit suites and quick gate. The
final scoped scale-range run caught all 16 generated mutations, and the
scale-selection run caught all 3 generated mutations, with no misses or
timeouts. B3 remains open.
