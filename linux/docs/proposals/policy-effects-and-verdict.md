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
- A script's audit class follows its first statement, so swapping `DELETE` and
  `INSERT` changes the recorded class (S8). Reproduced by B4.
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
functions such as `read_text`. Setting `enable_external_access=false` at
connection start would also break the driver's own flat-file flow, which opens
user-selected CSV, Parquet and JSON files through DuckDB reader functions. That
flow needs an explicit design (for example, allow only the selected path) before
external access is turned off. Until then DuckDB read-only relies on the policy
checks alone.
