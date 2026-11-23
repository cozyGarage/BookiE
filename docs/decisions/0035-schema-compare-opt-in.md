# 0035: Same-engine schema compare ships opt-in

- **Status**: Accepted (2026-11-04); amended 2026-11-23
- **Date**: 2026-11-04

## Context

Issue #2155 asks for a tables-first diff of two schemas on one engine, then a migration-style script that opens in the SQL editor for review. It also says to prefer dialect and DDL profile groundwork if that lands first. The 0.2.5 sprint plan (decision 6.3) kept compare out of the release until an ADR settled that groundwork.

#2409 merged the feature before either happened. `compare_schemas` in `crates/app-services` reads two schemas, builds the report, and produces the script with the same `diff_to_ops` and `materialize_ops` that the Structure tab uses. The report lists what is not compared: indexes, views, routines, triggers, sequences, grants and table options. The UI refuses when either schema read is truncated or skipped a table. A Docker test on PostgreSQL 18 compares two schemas, runs the generated script statement by statement and gets an identical second compare. A GTK scenario checks that the script opens in the editor and the database stays unchanged.

Because compare shares DDL generation with the Structure editor, the `diff_to_ops` signature change in #2376 and the compare caller in #2409 broke `linux` when they merged independently (fixed in #2430).

## Decision

Keep the code, and keep the entry hidden by default.

1. Compare stays same-engine, metadata-only and text-only. The script opens in an editor tab and runs only through the normal editor path: PolicyGuard, approval, audit, cancel and timeout. Nothing is applied automatically.
2. The schema row menu shows *Compare with another schema…* only when the environment variable `BOOKIE_SCHEMA_COMPARE` is set. Without it the feature is not reachable and is not advertised in release notes.
3. Dialect and DDL profile-as-data remains the prerequisite for removing the guard. A later ADR records that design; accepting it supersedes point 2.
4. #2155 stays open and is not a 0.2.5 deliverable.

## Consequences

- 0.2.5 users do not see schema compare. Maintainers and testers opt in with the variable.
- The Docker and GTK scenarios for compare set the variable; they keep guarding the behaviour that is already merged.
- Compare keeps the dialect coverage of the core DDL builders. It does not gain engine-specific diff rules before the profile ADR.
- Compare and the Structure editor now share one DDL path, so a change to `diff_to_ops` or `materialize_ops` must build and test both callers.

## Amendment (2026-11-23): row-level data compare

Issue #3818 adds a compare of the rows in two tables on the same engine, with a sync script for review. It lives next to schema compare in `crates/app-services` and follows the same rules, with these differences.

1. Data compare emits a script and never applies it. The script opens in an editor tab and runs only through the normal editor path: PolicyGuard, approval, audit, cancel and timeout. Nothing is applied automatically.
2. Data compare is visible without an environment variable. Point 2 still applies to schema compare only; `BOOKIE_SCHEMA_COMPARE` keeps guarding that entry.
3. Values compare as typed `bookie_core` values under [ADR 0007](0007-type-and-value-preservation.md), never as display strings. NULL equals NULL for the diff, and bytes compare as bytes. A column whose type has no exact comparison is reported as not compared.
4. Compare refuses instead of guessing. It needs a common primary key or user-chosen key columns that are NOT NULL on both sides. A duplicate key, a key that does not strictly increase in the stream, or a truncated read is an error.
5. Both sides stream in key order with a byte-order collation, so memory does not grow with table size. One cancel token stops both reads, and a cancelled compare produces no script.
6. Deletes in the generated plan start unselected; the user opts in before they enter the script.
7. Later stages, in order: range checksums with bisection behind a per-driver capability and a script batched by row count, then cross-engine compare with explicit lossy type-mapping warnings and a client-side sort with a spill cap.
