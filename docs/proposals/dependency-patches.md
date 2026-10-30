# Local dependency patches

BookiE maintains patched dependency snapshots under `vendor/`, applied through the workspace `[patch.crates-io]` entries. Each snapshot contains the base crate and the corresponding local patch file. `python3 scripts/vendor-patches.py check` verifies the snapshot against the registry crate plus the recorded patch.

## Local behavior

| Dependency | Behavior required by BookiE |
|---|---|
| PostgreSQL driver | Bound SCRAM work; preserve quoted text `NULL` in arrays; recognize multirange metadata for explicit refusal; resolve nested type metadata within a bound; support selected server-formatted text results; support protocol cancellation; expose session parameters used by SQL lexing. |
| MySQL driver | Preserve `ENUM` and `SET` text under binary collation; report `SET` column names; reject malformed or oversized packets with protocol errors. |
| MongoDB driver | Bound SCRAM work and verify the server nonce; preserve TLS hostname verification when connecting through a tunnel. |
| Redis driver | Preserve TLS hostname verification when connecting through a tunnel. |

Patch contents and base crate versions are listed in [`vendor/README.md`](../../vendor/README.md). Value behavior follows [ADR 0007](../decisions/0007-type-and-value-preservation.md); TLS identity and cancellation follow the local transport and driver contracts.

## Updating a patch

Keep each patch until a released dependency version provides the required behavior and BookiE's contract tests pass against it. Before updating a vendored crate:

1. Verify the released crate version and required behavior.
2. Compare edge cases and failure handling with BookiE's local contract.
3. Refresh the vendored snapshot and update the lockfile.
4. Run `python3 scripts/vendor-patches.py check` and the focused unit and real-engine tests.
5. Remove a patch only when its behavior is covered by the released crate and the tests pass.

An issue or proposed change alone does not establish that the behavior is present in a released dependency.
