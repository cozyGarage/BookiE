# Proposal: maintain local dependency patches

BookiE carries patches for selected database-driver dependencies under `vendor/`. Each patch addresses a boundary needed by the application, such as hostile-server resource limits, TLS identity, or exact native-value preservation. Patch files are checked by `scripts/vendor-patches.py check`.

## Removal rule

Keep a local patch until a released dependency version contains the required behavior and the replacement passes BookiE's contract tests. Before removing it:

1. Verify the exact released version and source change.
2. Compare edge-case behavior with the local patch, including failure handling.
3. Update the dependency and lockfile.
4. Run the focused unit and real-engine layers that exercise the contract.
5. Remove only the patch and vendor override that are no longer required.

An external issue or pull request is provenance, not evidence that the behavior has shipped in a release. Recheck its current status before acting.

## Patch inventory

| Behavior | Dependency | Reference or local contract |
|---|---|---|
| Bound SCRAM iteration cost and validate server nonces | PostgreSQL and MongoDB drivers | [SQLx report](https://github.com/transact-rs/sqlx/issues/4451), [MongoDB driver report](https://github.com/mongodb/mongo-rust-driver/issues/1819), [RUSTSEC-2026-0179](https://rustsec.org/advisories/RUSTSEC-2026-0179.html) |
| Preserve database service identity separately from a tunnel's dial address | MongoDB and Redis drivers | Local transport contract and TLS regression fixtures |
| Preserve quoted text `NULL` separately from SQL NULL in PostgreSQL arrays | PostgreSQL driver | [SQLx report](https://github.com/transact-rs/sqlx/issues/4452); ADR 0007 |
| Decode PostgreSQL multirange type codes | PostgreSQL driver | Local value contract and PostgreSQL oracle |
| Resolve deeply nested PostgreSQL domains and enum types | PostgreSQL driver | Local decoder regression cases |
| Preserve MySQL `ENUM` and `SET` text under binary collation | MySQL driver | [SQLx report](https://github.com/transact-rs/sqlx/issues/4453); ADR 0007 |
| Refuse malformed MySQL server packets instead of panicking or allocating without bound | MySQL driver | [SQLx pull request 4458](https://github.com/launchbadge/sqlx/pull/4458), merged on `main` 2026-10-16 and absent from 0.9.0; the hostile-server handshake test |

The MongoDB SCRAM change merged upstream as [#1833](https://github.com/mongodb/mongo-rust-driver/pull/1833) on 2026-10-21. Rechecked on 2026-10-27, MongoDB 3.9.1 remains the latest published crate ([release](https://docs.rs/crate/mongodb/latest)), so it does not contain that later merge and the vendored hunk remains required. SQLx 0.9.0 remains the latest published release ([release](https://docs.rs/crate/sqlx/latest)); its [PostgreSQL SCRAM](https://github.com/transact-rs/sqlx/issues/4451), [quoted NULL array](https://github.com/transact-rs/sqlx/issues/4452) and [binary-collation ENUM/SET](https://github.com/transact-rs/sqlx/issues/4453) reports were still open in the live issue tracker on 2026-10-27. Recheck release contents before removing any patch.

The links preserve issue provenance only. Current release status, patch contents, dependency versions and test selectors must be checked before changing `vendor/`.

The [October 28 peer dependency review](../archive/peer-dependency-review-2026-10-28.md) compares these patches and the direct dependencies with eight peer clients.

On 2026-10-28 two other unreleased SQLx changes were reviewed and not vendored. [Pull request 4394](https://github.com/launchbadge/sqlx/pull/4394) (a transaction cancelled during `BEGIN`) does not reach BookiE: its pool runs `DISCARD ALL` on release, which fails inside a transaction and closes the connection. [Pull request 4289](https://github.com/launchbadge/sqlx/pull/4289) (a pool maintenance CPU spin) changes `sqlx-core`, which BookiE does not vendor. Revisit both when SQLx publishes a release that contains them.
