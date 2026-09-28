# B3 external test-scenario survey

External sources below were first reviewed 2026-09-26 against BookiE `2eb9414c2`.
The local B3 status was reconciled 2026-09-28 through commit
`a90b86041`, including commits from September 26–28.
The upstream review sampled eight test files in four projects and two issue
reports; this local update is a source/test inventory, not a fresh execution of
every suite. No external source code or fixtures were copied.

## Focus to carry forward

- Finish B3 lossless values and consumer contracts before B4–B6 acceptance.
- Use upstream tests, issues and fix commits as bug hypotheses. Reproduce locally
  before changing production behavior; keep the reproducer as a permanent regression.
- Current coverage includes eight-driver scalar contracts, PostgreSQL scalar and
  temporal arrays, temporal eras/infinities/interval fields, MongoDB nested BSON
  consumers and SQLite NUMERIC-affinity transitions. B3 remains open for uncovered
  type/consumer combinations and installed grid acceptance; see the current status
  table and [value-contract evidence](value-contracts.md).
- Docker/local tests support B3. The GNOME VM supports later installed desktop
  acceptance and does not block this work.

## Inspected sources

- [dbeaver/dbeaver / PostgreValueParserTest.java](https://github.com/dbeaver/dbeaver/blob/50180e664c8b917e6a8865b43018a2f479806e5a/test/org.jkiss.dbeaver.ext.postgresql.test/src/org/jkiss/dbeaver/ext/postgresql/PostgreValueParserTest.java)
- [dbeaver/dbeaver / DataExporterCSVTest.java](https://github.com/dbeaver/dbeaver/blob/50180e664c8b917e6a8865b43018a2f479806e5a/test/org.jkiss.dbeaver.test.platform/src/org/jkiss/dbeaver/tools/transfer/DataExporterCSVTest.java)
- [beekeeper-studio/beekeeper-studio / all.js](https://github.com/beekeeper-studio/beekeeper-studio/blob/e90c46f4306475d58b0dadfdd441b865f2b5f1a1/apps/studio/tests/integration/lib/db/clients/all.js)
- [beekeeper-studio/beekeeper-studio / jsonb.spec.ts](https://github.com/beekeeper-studio/beekeeper-studio/blob/e90c46f4306475d58b0dadfdd441b865f2b5f1a1/apps/studio/tests/integration/lib/db/clients/postgres/jsonb.spec.ts)
- [dbcli/pgcli / test_pgexecute.py](https://github.com/dbcli/pgcli/blob/101e523eb2987ada87231c4533f0ab701c4c3124/tests/test_pgexecute.py)
- [dbcli/pgcli / test_sqlformatter.py](https://github.com/dbcli/pgcli/blob/101e523eb2987ada87231c4533f0ab701c4c3124/tests/formatter/test_sqlformatter.py)
- [sqlfluff/sqlfluff / lexer_test.py](https://github.com/sqlfluff/sqlfluff/blob/c7401613e851a6913ec3de8e003247e6fe8e5387/test/core/parser/lexer_test.py)
- [sqlfluff/sqlfluff / corpus_test.py](https://github.com/sqlfluff/sqlfluff/blob/c7401613e851a6913ec3de8e003247e6fe8e5387/test/core/parser/parity/corpus_test.py)

Commit-pinned links above identify the reviewed snapshots. Source ideas are
adapted to our contracts; verbatim code or fixture reuse needs recorded source
license and retained attribution before import.

## What the assertions teach us

| Project | Observed scenarios and flow | Application to BookiE |
| --- | --- | --- |
| DBeaver | PostgreSQL scalar/composite/array conversion; nested dimensions; NULL versus literal NULL; empty arrays and escaped braces, quotes and whitespace. CSV parameterizes separators, quoting and row content. | Add nested-value and consumer-format matrices. Verify server re-import as well as displayed text. |
| Beekeeper Studio | Shared driver suite with setup per scenario; metadata, stream counts/chunks/cancellation, mutation rollback and concurrent transactions. Separate read-only checks. | Extend the shared contract harness with explicit per-engine capability expectations. Session and read-only cases feed B4/policy acceptance. |
| pgcli | Real PostgreSQL execution then rendered output; binary, Unicode enum/JSON values, BC dates, timetz, intervals, large numeric strings, and batch stop/resume after errors. | Test the entire decode-to-consumer path. Use exact typed/server comparisons instead of relying on output substrings. |
| SQLFluff | Dialect fixture corpus parsed through multiple implementations; parity checks; lexer Unicode/source-span cases. Known divergence records must be removed when they unexpectedly pass. | Reuse one dialect corpus across splitting, parameters, formatting and policy classification, with consumer-specific expectations. |

Two inspected pgcli issue reports are linked directly by its regressions:

- [#1362](https://github.com/dbcli/pgcli/issues/1362): leading comments sent a special command through normal SQL dispatch.
- [#1403](https://github.com/dbcli/pgcli/issues/1403): a leading comment caused a multiline statement to execute incompletely.

These establish useful failure patterns, not confirmed BookiE bugs. BookiE does
not need pgcli backslash-command support to benefit from comment/dispatch tests.

The sampled Beekeeper common suite has capability returns and a runSoft helper
that catches some non-SQLite failures. Our gates must record exclusions and
unsupported outcomes explicitly; an assertion failure must fail its suite.
SQLFluff parser agreement is a useful differential signal, but agreement alone
does not establish server semantics or satisfy our authorization rules.

## Prioritized candidate matrix

Inventory status below comes from current local code/tests, not new executions.

| Priority | Candidate | Existing evidence / gap | Next test and oracle |
| --- | --- | --- | --- |
| B3-1 | PostgreSQL arrays | Scalar and temporal arrays preserve elements, NULL, dimensions and lower bounds as exact text; SQL INSERT, typed text input, JSON output and server wire equality have real-server contracts. Unsupported element OIDs are refused. Safe built-in casts make integer[] grid write-back pass with NULL preserved. The text[] contract checks NULL and escaped quote/backslash/comma through ordered unnest. The numeric[] grid contract checks wide precision, scale, NaN, ±Infinity and NULL with per-element `numeric::text` comparisons; both server grid contracts pass. | JSON array elements and automatic parameter typing remain unsupported/unverified. |
| B3-2 | Temporal boundaries | End-of-day time, timetz offsets, BC/extended-year SQL literals, infinities, mixed interval fields and temporal arrays have server-backed contracts. A PostgreSQL DATE at year 1,000,000 and TIMESTAMP at year 294276's finite upper bound are explicitly refused as `Undecodable` while independent `::text` oracles remain exact; the app parser rejects both values. SQL literal and parameter paths also refuse their undecodable markers. | Full support for finite dates/timestamps outside the shared chrono range and full non-SQL consumer/edit parity. |
| B3-3 | Nested JSON/BSON | MongoDB nested documents/arrays and uncommon top-level BSON kinds preserve special markers; Decimal128 extrema/date bounds and binary subtype tags have exact regressions, with server checks for UUID/user-defined binaries, large nested Int64, explicit null, Unicode, JSON/CSV/XLSX, Timestamp/regex/MinKey/MaxKey grid edits, canonical Extended JSON re-import and MCP browse output. Page metadata includes returned rows beyond its first-50 sample. A Docker-backed String/Decimal128 pair with identical text reports `mixed` metadata, retains native BSON kinds in storage, and the app gate refuses editing. | Generic scalar exports still cannot distinguish these two `Value::Text` values; collection-wide heterogeneity beyond sampled and returned-page documents and remaining top-level special BSON edits need acceptance. Keep SQL NULL distinct from JSON null. |
| B3-4 | Export/import consumers | SQL binary, SQLite NUMERIC-affinity storage-class re-import and policy-guarded CSV import, XLSX integers/decimals/temporal fallbacks, nested BSON markers, finite/nonfinite floats, XML text and CSV quoting have regressions; CSV float export/re-import preserves negative-zero bits through typed import, and XLSX stores negative zero as a signed numeric token. PostgreSQL app-parser output now reaches the keyed-update builder and live server for `NUMERIC(80,40)`, unconstrained `numeric` and NaN/±Infinity; a SQLite app parser/keyed-save contract verifies NUMERIC input to REAL storage. ClickHouse wide integers now have SQL export/re-import, CSV export/import with formula sanitization disabled, and grid edits across Int128 min/max and UInt128 zero/max adjacent values. XLSX explicitly refuses empty text; full format equivalence is unproven. | Installed grid acceptance, formula-safe CSV parity for text-backed wide integers, remaining floating-point edges and cross-format temporal parity. Parse generated files and re-import into typed columns where supported; document lossy format contracts. |
| B3-5 | Lexer/parser consumer agreement | A PostgreSQL CRLF script with issue-shaped leading, inter-statement, inline and trailing comments keeps its two executable statements in order across planner, named-parameter extraction, formatter and policy classification; SELECT/UPDATE classes, UPDATE target and WHERE survive formatting. The editor cursor contract maps GTK character offsets to byte offsets and selects the second statement after multibyte text. A malformed quoted tail reports a planner diagnostic, blocks whole-script execution, remains intact through parameter extraction and formatting, and classifies as unparseable/write. SQL Server GO batches retain three statements and batch policy through formatting, ignore placeholder-shaped delimiter comments, and classify SELECT/UPDATE/SELECT in order. MySQL DELIMITER directives survive planning and formatting, yield routine plus trailing query in order, and do not add parameters; the routine is unparseable and denied to agents by policy. | Broaden the executable-identity, malformed-tail and delimiter oracle across dialects; cover non-default MySQL delimiters and human approval behavior for unparseable routines. |
| B3/B4 | Result delivery and session state | The shared value path rejects incomplete rows instead of inventing NULL cells; driver cancellation and session tests also exist. A uniform delivery matrix is not established. | Zero-row metadata, duplicate column names, row-cap boundaries, multiple results, mid-stream failure/cancel and late results. Assert row order/count, completeness status and connection state. |
| B4 acceptance | Secure connection and authorization | TLS fixture crates and policy/MCP enforcement tests exist; this survey has not audited their full matrix. | Trusted/untrusted/expired certificates, endpoint identity through SSH, bad credentials, lost sessions, read-only operations, scopes/allowlists and audit outcomes. Explicitly map supported mechanisms per engine. |

Local anchors: `crates/drivers/postgres/tests/integration.rs`,
`crates/core/src/sql_lex.rs`, `crates/core/src/export/csv.rs`,
`crates/driver-tls-tests/tests/`, `crates/mcp/tests/enforce_policy.rs`.

## Test adoption flow

1. Record source project, exact revision, file/test and issue/fix link when available.
2. State the invariant, affected engines/types/consumers and expected failure mode.
3. Check existing local coverage; extend it or add the smallest missing case.
4. Establish expected behavior using engine/protocol documentation and a real
   server oracle. A different client is a comparison target, not the authority.
5. Run the reproducer against current code. Record reproduced, already covered,
   unsupported-by-design, not applicable, or blocked with the missing prerequisite.
6. Fix confirmed defects, rerun the reproducer and the relevant shared corpus.
7. Assign one existing regression tier and a runner; no orphan test files.
8. Keep exact value/type/scale/order assertions and negative cases. Add bounded
   property/fuzz cases for decoders and lexers; retain minimized failing seeds.

Each adopted case should carry: scenario ID, provenance, capability conditions,
input, expected value/type/metadata or explicit error, oracle, test location,
runner, baseline failure evidence and final passing revision. Do not hide failures
with broad retries, implicit skips, float-normalized comparisons or NULL fallbacks.

## Remaining survey work

- Inspect transport/authentication suites and fix commits in depth; only their
  inventory was sampled here. Include driver/protocol projects for wire behavior.
- Read the delegated Beekeeper helper assertions and CI entry points before
  adopting its stream/transaction test setup. The shared orchestration alone
  does not prove coverage across every engine.
- Review DBeaver statement-parser fixtures and the remaining SQLFluff dialect
  corpus selectively for our six SQL engines.
- B3-1 implementation follow-up: scalar and temporal arrays now preserve
  elements, dimensions and lower bounds through server wire, SQL and JSON checks;
  built-in `integer[]` grid write-back passes with NULL preserved. Broader array
  types and automatic parameter typing remain open. See the
  [array checkpoint](value-contracts.md#postgresql-array-checkpoint).
- B3-2 implementation follow-up: end-of-day time and timetz offsets have a
  [server round-trip contract](value-contracts.md#postgresql-time-checkpoint);
  temporal eras, infinities, mixed intervals and temporal arrays are also covered.
- Keep mutation testing alongside each decoder change and ensure its selected
  test filter includes malformed-input units as well as server regressions.
- B3-2 SQL export follow-up: BC dates and years above 9999 have a
  [server wire round-trip contract](value-contracts.md#postgresql-era-and-mutation-checkpoint),
  including repeated-hour instants. Values outside chrono's range, infinities,
  mixed intervals and temporal arrays still need their own acceptance.
- ClickHouse Int128/UInt128 now have Docker-backed exact-text contracts for
  query results, text binding, SQL export/re-import and keyed grid edits; other
  wide-integer grid boundaries remain open.
