# 0038: Key the audit journal's SQL hash

- **Status**: Proposed
- **Date**: 2026-11-04

## Context

`PolicyGuard` records `sql_hash` as an unsalted SHA-256 of the full statement (`crates/policy/src/guard.rs`). If a journal leaks, short statements or statements with low-entropy literals can be recovered by hashing candidates. Issue #2389. The journal's own record chain (`crates/storage/src/audit_journal.rs`) is a separate hash and is unaffected.

## Options

- **A. Keep unsalted SHA-256.** No code. Cost: the leak above stays.
- **B. Per-record random salt.** Recovery needs a guess per record, but the same statement no longer correlates across records, which breaks "how often did this statement run".
- **C. Per-install key, HMAC-SHA-256.** The same statement correlates inside one install; without the key a leaked journal cannot be matched against candidates. Cost: no correlation across installs, and the key must be stored and survive reinstall or the old hashes stop matching new ones.

## Recommendation

C. Store a random 32-byte key in the Secret Service next to the other secrets, with the existing storage-owner fallback when it is unavailable. Add `sql_hash_key_id` to new records so old unkeyed hashes stay readable and verifiable. Chain verification does not change.

## Consequences

- Policy and storage are B4-owned: the change needs the lane handoff.
- Acceptance: a test that no record stores `SHA-256(statement)`, a test that two identical statements share a hash under one key and differ under another, and a test that a journal holding old unkeyed records still verifies.
