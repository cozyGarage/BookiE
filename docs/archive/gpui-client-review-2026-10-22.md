# GPUI database-client lessons, October 22, 2026

## Scope and conclusion

Source inspection of eight suggested repositories found useful rendering, scheduling and PostgreSQL regression patterns. These patterns can be implemented within the existing GTK architecture. Framework selection alone does not establish bounded memory, correct cancellation or low CPU usage. No framework change is proposed.

The [source manifest](../evidence/gpui-client-review-2026-10-22/manifest.json) pins commits and observed licenses. This is a focused initial review, not a whole-codebase audit or a performance comparison. No third-party application was built or benchmarked. Findings describe inspected paths, not every path in a repository.

## Source findings

### zed-industries/awesome-gpui: Discovery index

The curated list identifies applications and components. Inclusion, stars and descriptions are discovery signals; they do not establish measured performance or suitability. Follow through to pinned source and licensing before adoption. [Pinned source](https://github.com/zed-industries/awesome-gpui/blob/ce5241772180adc575a84453fda37243fc878a2d/README.md).

### JosueRhea/dbui: Visible-row rendering and bounded previews

The result grid uses `uniform_list` and maps only the requested row range. `CELL_CHARS` is 200 and cell rendering calls `to_cell(CELL_CHARS)`. Staged edits are collected once per rendered range rather than repeatedly per cell. These reduce presentation work; they do not prove that underlying result storage is bounded. Check preview conversion cost separately from its output length. [Pinned source](https://github.com/JosueRhea/dbui/blob/6b98db2eeb01e4f2971b803383363993c4b99d7b/crates/dbui-ui/src/components/grid.rs).

### ShayanAbbas1/dbdelve: PostgreSQL cancellation has session consequences

The inspected query path reads simple-query messages and retains a limited result. Its stop-at-limit path checks activity on a side connection and otherwise drains results. The code explicitly accounts for cancellation aborting open transactions, side-session lock waits, truncated activity text and backend reuse behind transaction poolers. It also describes how preparing metadata in the user session can abort that transaction. These are valuable regression scenarios. The side-connection cancellation algorithm has documented identity limits and is not proposed for copying. [Pinned source](https://github.com/ShayanAbbas1/dbdelve/blob/b24b39947ed0c48f3bdd6eb9f839e8b82d6d47fa/src/db/postgres.rs).

### 0xErwin1/dbflux: One warning handoff across result consumers

Dedicated handoff functions cover SQL editor, table browse, collection browse, visual queries, mutation previews and CRUD returning results. The SQL editor path distinguishes row-limit footer information from warnings. This suggests a consumer matrix for BookiE: every result warning must remain observable exactly where it matters, without silently disappearing during an adapter conversion. [Pinned source](https://github.com/0xErwin1/dbflux/blob/ce72a48a6144ee36474862f99f148c50bffa4c70/crates/dbflux_ui_document/src/result_warnings.rs).

### tobi/disktree: Admission limits separate active and pending work

`Admission<T>` keeps pending jobs outside active worker slots and admits only the available capacity. Completion returns capacity; changing the limit increments an epoch used by samples. This is a useful model for expensive catalog or preview work. Its pending `Vec` is not itself byte-bounded. Do not interpret worker admission as a total-memory guarantee or import its adaptive governor without a measured need. [Pinned source](https://github.com/tobi/disktree/blob/6c8d4ce6211bf135ff4e42890faebff1040e1846/crates/disktree-core/src/scan_threads.rs).

### pavi2410/based: Browse pagination is distinct from query execution

The browse helper performs COUNT and LIMIT/OFFSET page fetching, then converts cells for display. A count failure becomes zero in this path. This is a counterexample for explicit outcomes: an unavailable count should not become a genuine zero. Pagination alone also does not establish stable ordering or constant cost at large offsets. BookiE should preserve exact values and distinguish unknown totals. [Pinned source](https://github.com/pavi2410/based/blob/db046228e3036212ea6dc4fccdc03bd2bcc475c4/crates/based-postgres/src/page.rs).

### fluxdb-alt/fluxDB: A query tab carries session identity

The request uses `QuerySessionId(tab_id.0)` and a tab-associated cancellation flag. Success updates result editors and history; failure clears result editors and records a user-facing error. This reinforces testing session continuity and terminal-state cleanup across repeated execution. The code also clones execution results into editor state; this inspection does not establish the allocation cost of those clones. [Pinned source](https://github.com/fluxdb-alt/fluxDB/blob/017c2524b90f502890803942d85d65ca11de326a/crates/fluxdb-app/src/parts/controller/query_execution.rs).

### feigeCode/navop: Bounded defaults and explicit result envelopes

`ExecOptions::default` specifies 1,000 rows and 30 seconds, with a regression assertion. `RowBatch` carries columns, rows and an optional next cursor. Defaults are useful boundary tests, but optional limits do not prove enforcement, and this small value enum cannot replace BookiE's exact type contract. The repository contains a custom NAVOP_LICENSE with redistribution and competing-product restrictions, alongside Apache licensing material. No code reuse is proposed; file-level permission would need clarification first. [Pinned source](https://github.com/feigeCode/navop/blob/28ab54c8d7cfd28b1f470e65101165bcbe4c329c/crates/extension-api/src/db.rs).

## Adoption order and acceptance

| Priority | Candidate | Existing BookiE coverage or work | Owner and required evidence |
| --- | --- | --- | --- |
| P1 | Bound presentation work as well as retained data | `stream_pages.rs` retains four pages. Hidden-grid lifetime tests and PR #1309 address construction timing. This does not bound bytes within a page. | UX: profile narrow and wide cells, many columns, rapid page replacement and closed tabs. Assert discarded grids never build and release their result references. Compare CPU and live allocations separately from RSS. |
| P1 | PostgreSQL session and cancellation regression matrix | ADR 0005 governs interruption; PR #1304 repairs terminal-page metadata consistency. | B3/B4: real PostgreSQL tests for open transaction plus row limit, writes with RETURNING, metadata-description failure, cancellation during a lock wait, and pooler identity. Verify persisted state and usable or explicitly retired sessions. Do not assume these cases are currently broken. |
| P1 | Warning preservation across consumers | ADR 0007 requires explicit unsupported values; current pager communicates retained rows. | B3/UX: exercise query, browse, export, MCP and mutation preview with truncation and unsupported values. Assert observable warning and exact-value/refusal behavior. Inventory existing selectors before adding tests. |
| P2 | Bound metadata and preview work admission | Generation IDs and headless app-services already provide relevant application boundaries. | UX: inspect existing SingleFlight/request ownership first. Test closing a tab and changing connections while work is queued. Bound queue payloads as well as workers. Add no scheduler until a profile identifies redundant work. |
| P2 | Distinguish unknown counts from zero | No BookiE count defect was established by this review. | B3/UX: trace browse totals and test count failure separately from an empty table before filing a defect. Preserve stable ordering for page fetches. |

The existing performance issues #1272, #1299 and #1306 remain the appropriate work records. This review does not establish additional BookiE bugs, so it creates no duplicate defect issues.

## Reuse boundary

No source was copied and no dependency added. MIT and Apache labels are starting points for file-level provenance, notices and dependency review. GPL-labeled sources require checking the actual license version and compatibility before incorporation. Navop's custom restrictions prevent treating its repository as generally reusable open-source code. The curated list's license does not license the linked projects.

The preferred first step is an independently implemented regression case or a small correction to an existing GTK path. A GPUI prototype would require a separate product decision and evidence covering accessibility, input methods, clipboard, large text, display scaling and packaging. None of those capabilities was verified here.
