Patched crates.io snapshots used through `[patch.crates-io]` in the workspace
`Cargo.toml`. Refresh by copying the same crate version from crates.io and
re-applying only these edits:

- `sqlx-postgres` 0.9.0: reject SCRAM PBKDF2 iteration counts above 100_000 in
  `src/connection/sasl.rs` (same cap as postgres-protocol 0.6.12 / RUSTSEC-2026-0179).
- `mongodb` 3.9.1: reject SCRAM iteration counts above 100_000 in
  `src/client/auth/scram.rs`.
