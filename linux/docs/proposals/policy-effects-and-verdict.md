# Proposal: statement effects and a joined verdict

Status: proposed, for the B4 lane. Not a decision; an ADR follows if accepted.

## Problem

A review on 2026-10-08 found that the policy outcome depends on the order in
which things are written, not only on what a statement does:

- Rules run first-match. An early "unparseable allows" or categorical rule can
  skip a later write-approval rule (S6).
- Human approval is skipped for some administrative and destructive statements
  in local and staging (`COPY ... TO PROGRAM`, `MERGE ... DELETE`, `TRUNCATE`)
  (S7).
- A script's audit class follows its first statement, so reordering the same
  statements changes the class (S8).
- `classify.rs` and the masking walker traverse the same AST separately and have
  drifted (a write the classifier sees, masking misses).

These are verified only as far as the review's probes; B4 should reproduce each
before relying on it.

## Proposal

1. **Effects as a set.** A statement's facts become a bit set (`READS`, `WRITES_ROWS`,
   `WRITES_SCHEMA`, `ADMIN`, `SESSION_STATE`, `FILE_ACCESS`, `UNKNOWN`). A script's
   effects are the union of its statements', so order cannot change them.
2. **Verdict as a join.** Each rule returns `Allow`, `RequireApproval` or `Deny`
   with a reason. The decision is the maximum under `Allow < RequireApproval < Deny`,
   with reasons concatenated. Rule order cannot change the outcome.
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
