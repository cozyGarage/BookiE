# 0010: Administrative events use the existing audit record

- **Status**: Proposed
- **Date**: 2026-10-07

## Context

Every statement that reaches a database is audited as an intent and an outcome through `PolicyGuard`. Four actions change or expose state without running a statement, and none of them writes an audit record today:

- opening an SSH tunnel and refusing or accepting a host key (B4-7);
- the table read that Connect and Test Connection run on a raw connection before the guarded handle exists (B4-18);
- exporting and importing a connection bundle (B4-22).

`LIST TABLES` and `LIST DATABASES` already show the pattern for non-SQL operations: the guard writes an `Administrative` event whose `redacted_sql` is a fixed label.

## Decision

Administrative events reuse `AuditEvent`. No new journal format.

- `operation_class` is `Administrative`. `redacted_sql` is one fixed label: `TUNNEL SETUP`, `HOST KEY DECISION`, `CONNECT SCHEMA READ`, `BUNDLE EXPORT` or `BUNDLE IMPORT`. `sql_hash` is the hash of that label.
- Each action writes one intent and exactly one terminal outcome: `Succeeded`, `Failed`, `Denied` (a host key refusal or a declined import), `Cancelled` or `TimedOut`.
- `targets` names the saved connection or the file name only. Passwords, passphrases, key material, bundle contents and host key fingerprints are never recorded.
- A bundle action has no connection, so it uses the nil connection id and the connection name `(bundle)`.
- The connect-time schema read moves behind the guard, so it becomes an ordinary audited `Read` and the raw read is removed. This is the preferred fix for B4-18.
- An audit write failure denies the action, as for statements.

## Rationale

The journal, its readers, retention and the terminal-state invariants already exist and are tested. A second format would need its own migration under ADR 0009 and its own readers. Fixed labels keep every record non-secret by construction.

## Consequences

- `AuditOperationClass` needs no new variant; the journal stays readable by older builds.
- Tunnel and bundle code gain an audit dependency. `transport` must not depend on `storage`, so the GUI and `agentd` composition roots pass an `AuditSink` into tunnel setup.
- Each new label needs allowed and denied tests and a terminal-state test, as for statements.
- The label list is a closed set. A new administrative action needs a new label and a test.

## Alternatives considered

- A separate administrative journal: rejected, two formats to read, retain and migrate.
- Log lines only: rejected, logs are not the tamper-evident record and have no terminal-state guarantee.
- A new `AuditOperationClass` variant per action: rejected, it changes the serialized enum and breaks older readers for no gain over a label.
