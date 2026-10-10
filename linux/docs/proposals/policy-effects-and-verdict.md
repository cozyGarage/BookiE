# Proposal: statement effects and a joined verdict

Status: proposed, for the B4 lane. Not a decision; an ADR follows if accepted.

## Problem

A review on 2026-10-08 found that the policy outcome depends on the order in
which things are written, not only on what a statement does:

- Rules run first-match. With `human_approve_unparseable=false`, a malformed
  `DELETE; ...` is allowed even when write approval is on (S6). Reproduced by B4
  on the `linux` tip.
- Human approval is skipped for some administrative and destructive statements
  (S7). Reproduced in part: `COPY ... TO PROGRAM` and `MERGE ... DELETE` are
  allowed in both Local and Staging; `TRUNCATE` is allowed in Local but needs
  approval in Staging.
- A script's audit class depends on write-statement order: `DELETE; INSERT` is
  classified as `Insert`, while `INSERT; DELETE` is classified as `Delete` (S8).
  An administrative class dominates. Reproduced by B4.
- `classify.rs` and the masking walker traverse the same AST separately and have
  drifted (a write the classifier sees, masking misses).

S6 and S8 are confirmed; S7 is confirmed for the statements named above.

## Proposal

1. **Effects as a set.** A statement's facts become a bit set (`READS`, `WRITES_ROWS`,
   `WRITES_SCHEMA`, `ADMIN`, `TRANSACTION_CONTROL`, `SESSION_STATE`, `HOST_OR_FILE_ACCESS`,
   `UNKNOWN`). A script's effects are the union of its statements', so order cannot
   change them.
2. **Verdict as a join.** Each rule returns `Allow`, `RequireApproval` or `Deny`
   with a reason. The decision is the maximum under `Allow < RequireApproval < Deny`,
   with reasons concatenated. Rule order cannot change the outcome. Each reason
   keeps its stable rule name, so the audit event still says which rule decided.
3. **One traversal.** Classification, masking and blast-radius planning read the
   effects from a single AST walk.

## Migration

One mechanism per PR, each with a test that reorders its inputs and expects the
same result:

1. Add `Effects` beside the current facts; compute both; assert equal in tests.
2. Move scripts to the union and drop first-statement classification (S8).
3. Convert rules to return verdicts and fold them; keep rule names for audit
   (S6, S7).
4. Delete the old facts fields and the duplicated walkers.

## Checks

- Property test: any permutation of a script's statements yields the same
  effects and the same verdict.
- Every existing `Decision` rule name still appears in the audit event.
- The agent and human paths, read-only connections and approval timeouts keep
  their current outcomes (the existing policy tests are the oracle).

## DuckDB read-only (S3)

A read-only DuckDB connection should not read arbitrary local files through
functions such as `read_text`. The driver also supports opening a user-selected
CSV, TSV, Parquet or JSON file as a view in an in-memory DuckDB connection. The
read-only design must preserve that flow while denying access to every other
external path.

For a flat-file connection, resolve the selected local path to a canonical
absolute path, require a regular file, and pin its identity before DuckDB opens
it. A path allowlist alone is not an identity allowlist: after the selected
path is replaced, DuckDB can still authorize a different file at that same
path. The Linux driver pins the opened file and creates a private hard link to
that inode on the same filesystem. DuckDB's `allowed_paths` contains only this
private alias; the driver creates its view from that alias and retains the
alias directory for the connection lifetime. Direct external reads through the
user-selected path and all sibling or unrelated paths must fail, including
after the user-selected path is replaced. If identity pinning or the exact
allowlist cannot be applied, refuse the connection rather than opening it with
external access enabled. DuckDB documents `allowed_paths` alongside
`enable_external_access` in its [file access security
controls](https://duckdb.org/docs/current/operations_manual/securing_duckdb/overview).

For a `.duckdb` or `.db` connection, open the selected database in read-only
access mode (the pinned Rust API exposes this through
[`Config::access_mode`](https://docs.rs/duckdb/1.10505.0/duckdb/struct.Config.html#method.access_mode))
and disable external access before handing the connection to the query path.
The read-only policy remains responsible for rejecting SQL writes. The driver
must confirm that opening the primary database still works with the
external-file restrictions enabled.

The pinned Rust wrapper applies the list-valued setting through trusted
initialization SQL before user SQL, using a tested SQL-literal encoder. This is
a DuckDB-level path restriction, not an operating-system sandbox. A private
same-filesystem hard link preserves the selected inode across pathname
replacement; if the filesystem cannot provide that link or a private directory
on the same filesystem, the read-only flat-file connection must fail closed.

Acceptance tests must show that the selected file can populate its view, while
`read_text`, `read_csv`, `read_parquet`, `read_json`, `ATTACH` and `COPY` cannot
access a sibling, unrelated path, or the original selected path directly.
Include quotes and Unicode in selected paths, a symlink/path-alias case,
replacement races during setup and after connection, attempts to change the
settings after configuration is locked, and mutation attempts against a
read-only database. Do not close this control until those tests pass on the
pinned bundled DuckDB version and the required acceptance gate completes.
