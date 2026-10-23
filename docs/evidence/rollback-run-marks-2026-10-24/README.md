# Run markers after confirmed rollback

Related: [#1490](https://github.com/cozyGarage/BookiE/issues/1490), rollback-marker finding only.

## Cause and change

A completed editor run adds gutter marks. A later confirmed session rollback changed the session status but left those previous marks visible. The same omission existed when COMMIT reported a rolled-back terminal outcome.

Both matching confirmed rollback paths now clear prior run marks. Stale session callbacks still return before changing UI state. A successful commit and unconfirmed errors retain their existing behavior. No database, audit, policy or stored-result behavior changes.

## Regression

The isolated editor fixture opens a transaction, runs a query and requires a visible run mark. It checks that a stale rollback callback preserves the mark and current session, then delivers the matching completion and requires the mark to disappear. It covers explicit rollback and a commit completion reporting RolledBack.

The test failed on unfixed code with “confirmed rollback left the prior run marker visible.” It passes after the fix with fatal GTK criticals enabled. An initial probe omitted the fixture's session-ending state; that setup was corrected before collecting the failing regression. The test is registered under gtk in scripts/isolated-tests.json; the ignored-test inventory check verifies that registration.

The fixture tests terminal UI message handling. It does not establish database rollback semantics, persistence of server-side effects or native Wayland appearance.

## Validation

Full preflight, strict application Clippy and the complete GTK tier are pending. No guardrail, timeout or transaction outcome was weakened.
