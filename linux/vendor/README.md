Patched crates.io snapshots used through `[patch.crates-io]` in the workspace
`Cargo.toml`. Refresh by copying the same crate version from crates.io and
re-applying only these edits:

- `sqlx-postgres` 0.9.0: reject SCRAM PBKDF2 iteration counts above 100_000 in
  `src/connection/sasl.rs` (same cap as postgres-protocol 0.6.12 / RUSTSEC-2026-0179),
  preserve quoted `NULL` text elements in text arrays in `src/types/array.rs`,
  and recognize PostgreSQL multirange type codes in `src/connection/resolve.rs`
  so unsupported scalar and array values reach BookiE's explicit refusal path.
- `mongodb` 3.9.1: reject SCRAM iteration counts above 100_000 in
  `src/client/auth/scram.rs`, and let the TLS layer verify a hostname
  different from the one it dials, in `src/client/options.rs` and
  `src/runtime/tls_rustls.rs`.
- `redis` 1.7.0: let a TLS connection verify a hostname different from the
  one it dials, in `src/connection.rs`, so a certificate check survives an
  SSH tunnel's loopback dial address.
