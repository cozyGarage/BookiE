# 0056: Audited reconciliation of unknown write outcomes

- **Status**: Proposed
- **Date**: 2026-11-19

## Context

Issue [#947](https://github.com/cozyGarage/BookiE/issues/947) records a gap left by [ADR 0008](0008-connection-and-session-ownership.md) and [ADR 0016](0016-governed-consumer-seam.md). After a write whose server-side result cannot be proven (a lost acknowledgement, a tunnel drop mid-COMMIT, a crash between intent and outcome), the audit journal holds a durable `Unknown` outcome. Today:

- `track_operation_state` in `crates/storage/src/audit_journal.rs` keeps an operation unresolved until a later `Outcome` for the same `operation_id` is non-`Unknown`. Recovery after a crash appends such an `Unknown` outcome for every pending intent (`recovery_outcome`).
- `reject_new_write_intent` refuses a new write intent with `UnresolvedWrite` when an unresolved write exists for the same saved connection, or when either side is the nil connection ID (process-wide). Rollbacks are exempt. The guard turns this into a denied operation and disables governed writes for that connection generation.
- The GUI seeds the affected connection as blocked after restart. `bookie-agentd` refuses startup only for a global unresolved write.
- Reconnect, retry and restart never clear it (ADR 0008: "Reconnect cannot erase an audit intent or prove whether the server committed a write").

No operator action exists to say "I checked the server; this write did commit" or "it did not". The affected connection stays write-blocked indefinitely, and the only workaround is to edit or delete the journal, which is exactly what the journal exists to prevent. Issue #1016 is a reachable case (an Unknown ROLLBACK after tunnel loss in a Session transaction). PR #951 scoped the block per connection; PR #3132 stopped known-not-sent failures from creating a false Unknown. Neither added recovery.

The maintainer has decided that an explicit, audited resolve path must exist. This record defines it.

Threat model, for honesty: the journal chain is unkeyed SHA-256 over previous hash, sequence and event bytes (issue #947 comments). It detects corruption and partial edits. It does not stop a process running as the same OS user from rewriting the whole file. This ADR does not change that guarantee and does not claim the resolve path is tamper-resistant against such an actor.

## Decision

1. **Resolve is an explicit human act.** An operator may record a decision for one unresolved `Unknown` write outcome: `committed`, `rolled_back`, or `still_unknown`, with a mandatory reason. Nothing else clears an unresolved outcome. Reconnect, retry, restart, generation replacement, a later success on the same connection, and "resolution by timeout" never do.
2. **The original record is never modified.** The `Unknown` outcome (and the intent before it) stay as written. A new reconciliation record is appended, referencing the original `operation_id`.
3. **Who may resolve.** Only a `Principal::Human` acting through a local interactive entry point (below). There is no MCP tool, no agentd RPC, and no agent-token path. The journal API refuses a reconciliation whose principal is an agent, and chain verification treats such a record as corruption. MCP scope tokens (ADR 0016's first check) do not gain a scope for this; the capability is absent, not gated.
4. **Entry points (intent only).**
   - GUI: where the UI already shows that a connection is write-blocked by an unresolved outcome, offer "Resolve outcome". It shows the original record (time, operation class, redacted SQL, error, token if the driver recorded one per ADR 0051), asks for one of the three outcomes and a reason, and requires explicit confirmation. It states that the choice is the operator's claim, not proof, and that BookiE will not replay or re-check the write.
   - Headless CLI: a local, non-MCP command (name to be chosen at implementation) that lists unresolved operations and resolves one by `operation_id`. It is the only route for a global (nil connection) unresolved outcome, because `bookie-agentd` refuses to start in that state and the GUI may not be running. It opens the journal directly under the existing file locks and never starts a server or an MCP listener.
5. **Record schema.** The existing `AuditEvent` gains one optional, additive field, following the precedent of `transport_attempt` and `administrative_action`:

   ```text
   reconciliation: Option<AuditReconciliation>
     reconciles_operation_id: Uuid     // the Unknown operation being decided
     claim: committed | rolled_back | still_unknown
     reason: String                    // required, trimmed, 1..=1000 chars, control chars rejected
     evidence: none | commit_token_checked | server_checked_manually   // operator-declared, optional
   ```

   The carrying event is a single `Outcome` record with a fresh `operation_id`, `operation_class: Administrative`, `terminal_status: Succeeded` (the reconciliation itself succeeded, which also keeps older readers from treating it as a new Unknown), the original's `connection_id`, `connection_name`, `environment` and `driver_id`, the resolving human principal, and `redacted_sql: "[NOT_APPLICABLE]"`. It is appended durably (fsynced) like intents and administrative actions. It is one record, not an intent/outcome pair: there is no external side effect, and the record is the effect.
   The reason is free text and may be read by anyone with the journal; the UI warns against pasting secrets and the journal refuses values over the cap. No SQL text or values are added beyond what the original record already holds.
6. **Admission rules in the journal** (inside `append_locked`, under the existing exclusive file lock, so GUI and CLI cannot race):
   - The referenced operation must currently be unresolved with a recorded `Unknown` outcome, and its `connection_id` must equal the record's. A pending intent (still in flight in a live process) is not resolvable; refuse with `WriteInFlight`.
   - A second `committed`/`rolled_back` for an already resolved operation is refused. `still_unknown` may be recorded repeatedly (an updated note) and never clears the block.
   - A write intent for the connection is admitted only when every unresolved operation for that connection (and every global one, for a non-nil connection) has a `committed` or `rolled_back` reconciliation. `track_operation_state` gains one arm: a valid reconciliation with those claims removes the referenced operation from `unresolved`; `still_unknown` leaves it.
7. **Chain and verification.** Hash and sequence rules are unchanged: the record is hashed over its exact bytes like any other, so `verify_chain`, `recent`, and cursor resumption need no new format version. Verification adds semantic checks and treats a violation as chain corruption (fail closed, journal-wide): reconciliation by an agent principal, an empty or oversize reason, a reference to an operation that was not unresolved at that point in the chain, or a connection mismatch. Verification still must not rewrite or repair anything.
8. **What happens to the blocked connection.**
   - `committed` or `rolled_back` (and no other unresolved operation remains for it): the journal stops refusing writes for that connection. Live in-memory `AuditState` flags are sticky by design (ADR 0008: stale generations cannot clear a replacement), so the owning composition root clears the block only after the reconciliation record is durable, by creating a new connection generation seeded from the journal (the approved connection restart path), never by flipping the old guard. Another process (for example a running agentd) keeps its own flag until it replaces that connection's generation; that is fail-safe.
   - `still_unknown`: the connection stays blocked. Reads continue to work as they do now.
   - Nothing is retried or replayed. A lost dedicated Session stays unusable (ADR 0008). The operator's claim changes admission, not database state; if `committed` was claimed and the operator wants the change again, that is a new governed write with its own approval.
   - Other connections are unaffected. A global unresolved outcome blocks every connection until resolved.
9. **Failure modes.** All fail closed and leave the block in place.
   - Journal unwritable, fsync failure, or short write: the existing append path truncates the unconfirmed record; the resolve call returns an error; no in-memory state changes; the UI shows that the connection remains blocked and why.
   - Journal corrupt or unavailable: resolve is refused. Integrity failures are journal-wide and are not resolvable through this path (ADR 0008).
   - Process crash after the record is durable but before in-memory clearing: on next start the journal derivation already shows the operation resolved; correct. Crash before durability: nothing recorded; still blocked.
   - Concurrent resolves: serialized by the file lock; the loser gets "already resolved" with the first record's details.
   - A resolve attempted from a non-human or unknown principal: refused and not recorded as a resolution.
10. **Migration and compatibility.** No migration. Existing journals contain unresolved `Unknown` outcomes and no `reconciliation` field; they parse unchanged, hash unchanged, and become resolvable. The legacy `phase1` rotation is untouched. `AuditEvent` has no `deny_unknown_fields`, and the field is `#[serde(default, skip_serializing_if)]`, so an older binary reading a newer journal still verifies the chain and still sees the operation as unresolved (the carrier is a non-Unknown Administrative outcome for a new operation ID). Downgrade therefore re-blocks a resolved connection: safe, never silently permissive. `AuditJournalRecovery` grows reconciliation information so the GUI can show history.
11. **Required tests** (at implementation):
    - Storage: resolve each claim; original `Unknown` bytes unchanged; chain verifies after; `recent()` returns the record; unresolved set and `unresolved_connection_ids` update only for committed/rolled_back; `still_unknown` keeps the block; two unresolved operations need two resolutions; reference to unknown, resolved, pending or other-connection operation refused; oversize/empty/control-character reason refused; agent principal refused at append and flagged at verify; tampering with a reconciliation record or the original breaks verification; unwritable journal and fsync failure (extend the existing injected-sync test) leave state unchanged; two concurrent resolvers produce exactly one resolution; cursor resume across a reconciliation; old-format journal fixture opens and resolves; a fixture with the new field read by the previous record shape (extra field ignored, still blocked).
    - Policy: after resolve and a new generation, a write intent on that connection is admitted and an unrelated connection was never blocked; the old guard instance stays blocked; reconnect, retry and restart without a resolve stay blocked.
    - App services / agentd: no MCP tool or agentd method exposes resolve (assert against the tool and RPC listing); agentd start with a global unresolved outcome still refuses until the CLI resolves it; GUI action is disabled for agent-derived sessions by construction.
    - Manual checklist entry for the installed GTK flow, including journal-unwritable behaviour.

## Rationale

The journal's job is to preserve what BookiE knew when it was uncertain. An operator can know more (they can query the server, check a transaction ID, or read the database). Recording that knowledge as a new, attributable, durable fact keeps both truths: the system was uncertain, and a named person later asserted an outcome with a reason. Preserving the original and appending matches the chain's append-only design and the recommendation in the issue thread. An additive optional field avoids a format bump and degrades safely under older readers. Restricting the capability to a human at a local interface keeps the control away from the agent that might otherwise unblock itself, consistent with ADR 0016 (an MCP path cannot widen a human decision) and ADR 0008 (no implicit clearing).

## Consequences

- An operator has a documented way out of a permanent write block without touching the journal file.
- The resolution is an unverified claim. A wrong `committed` or `rolled_back` is possible and is recorded with who said it and why; BookiE does not detect it. The UI must say so, and `evidence` records whether a token check was done.
- One more record type to verify, display, and test across storage, policy, app services, the GUI and a new CLI command; two surfaces must stay in step.
- Cross-process visibility is by journal, not by live push: another running process keeps its block until it replaces the connection generation.
- The unkeyed-chain limit is unchanged: someone with write access to the file as the same user can forge a resolution. This ADR does not add an authenticated checkpoint. That, and the append-time full-verification cost tracked in #947, remain separate decisions.
- ADR 0008's "Uncertainty scope" paragraph ("Recovery uses the approved connection restart path") needs a one-line amendment pointing here if this is accepted.

## Alternatives considered

- **Leave the connection blocked (status quo).** Rejected. The only recovery is editing or deleting the journal, which destroys the audit evidence, and #1016 shows ordinary network faults reach it. Safe, but it pushes operators to bypass the control.
- **Clear on reconnect, retry or restart.** Rejected (ADR 0008): a reconnect cannot prove whether the server committed.
- **Auto-resolve by probing the server (commit token or query log) with no human.** Rejected as the only path: not every driver supports it (ADR 0051 covers PostgreSQL and ClickHouse), and a probe result still needs recording. A checked token may be attached as `evidence`, and automatic probing can be a later, separate decision that appends the same record type with a non-human principal only if a new ADR allows it.
- **Rewrite or replace the original Unknown outcome with the resolved status.** Rejected: loses the uncertainty record and breaks the append-only chain.
- **A new `AuditRecordPhase::Reconciliation`.** Rejected. An older binary would fail to deserialize it and refuse the whole journal, which turns a downgrade into a journal-wide outage instead of a safe re-block.
- **Reuse a non-Unknown `Outcome` with the original `operation_id`.** Rejected: that is the implicit clearing path itself, indistinguishable from a real driver outcome and not attributable to a person.
- **Delete or truncate the record, or a "reset journal" command.** Rejected: destroys evidence and the hash chain.
- **An agent or MCP resolve tool, even with approval.** Rejected: the agent would be unblocking its own writes. Human at a local interface only.
- **Time-based expiry of the block.** Rejected: time does not establish the outcome.
- **Resolve only through the GUI.** Rejected: a global unresolved outcome keeps agentd from starting and a headless host may have no GUI.

## Open questions

- Operator identity: `Principal::Human` carries only a session label. Whether to also record the OS user name needs a privacy and usefulness decision.
- Whether global (nil connection) outcomes should be resolvable at all in the first slice, or CLI-only as drafted.
- Whether `committed`/`rolled_back` should require an `evidence` value in `prod` environments, or a second confirmation.
- CLI name and home (new binary, `bookie-agentd` subcommand, or app-services entry).
