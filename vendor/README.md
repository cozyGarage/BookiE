Patched crates.io snapshots used through `[patch.crates-io]` in the workspace
or experiment `Cargo.toml`. Each directory is the pristine crate plus one patch file in
[`patches/`](patches/); nothing else may differ. `python3 scripts/vendor-patches.py
check` downloads each pristine crate, verifies it against the registry checksum,
applies the patch and fails on any difference, so an undocumented edit cannot
hide in a vendored tree.

Update a crate to a new release with
`python3 scripts/vendor-patches.py refresh <crate> <version>` (it re-applies the
patch and stops if a hunk no longer fits), then `cargo update -p <crate>`. After
changing a vendored file by hand, run `python3 scripts/vendor-patches.py generate
<crate>` and commit the new patch.

| Crate | Version | What the patch changes |
|---|---|---|
| `sqlx-sqlite` | 0.9.0 | Widen the `libsqlite3-sys` dependency range through 0.39.x, matching SQLx pull request 4502 so the fixed binding release can be selected. |
| `sqlx-postgres` | 0.9.0 | Keep up to 256 server notices per connection, each cut to 8 KiB, and hand them to the caller through `take_server_notices` with a count of the notices dropped. Reject SCRAM iteration counts above 100,000 and a server nonce that does not extend the client nonce (same cap as postgres-protocol 0.6.12, RUSTSEC-2026-0179). Keep quoted `NULL` text in arrays. Recognise multirange type codes so unsupported values reach BookiE's explicit refusal path. Resolve type metadata up to 1,024 levels so deep domain and enum chains load. Allow a query to request text results for selected server-formatted scalar types. Send protocol CancelRequest packets using a connection's frontend backend key for pooler-safe cancellation. Expose the server's last reported run-time parameters (such as `standard_conforming_strings`) so the editor lexes SQL the way the session does. Two build fixes for the trimmed feature set (a `Json` import path and a dead-code expectation). Enforce the cleartext authentication opt-in for the initial handshake as well as authentication switches. Turn on TCP keepalive (30 s idle, 10 s interval) on the direct socket. Stop sending `TimeZone=UTC` in the startup message so a session keeps its role or server time zone. |
| `sqlx-mysql` | 0.9.0 | Count the warnings that OK and EOF packets report and expose the count since the last call through `take_warning_count`. Report `ENUM` and `SET` columns as text even when a collation sets the binary flag, and name `SET` columns. Return protocol errors instead of panicking or allocating without bound on malformed server packets: handshake, ERR, EOF, text and binary rows, the authentication continue packet, a result set claiming more than 4,096 columns, and a packet reassembled from more than 1 GiB of parts (the changes of SQLx pull request 4458, which is merged on SQLx `main` but not in 0.9.0). Enforce the cleartext authentication opt-in for the initial handshake as well as authentication switches. Turn on TCP keepalive (30 s idle, 10 s interval) on the direct socket. Refuse the in-band RSA password exchange (`caching_sha2_password` full authentication and `sha256_password`) when the connection is neither TLS nor a local socket, so the password is never encrypted to a public key a network observer could have substituted. A connection through BookiE's SSH tunnel dials a local socket, so it passes this check although only the SSH leg is encrypted: the password is protected as far as the SSH server, and the last hop to the database is the server operator's to secure. Whitespace only elsewhere. |
| `tiberius` | 0.13.0 | Keep up to 256 informational messages (`PRINT`, `RAISERROR` at severity 10 or below) per connection, each cut to 8 KiB, and hand them to the caller through `Client::take_server_info` with a count of those dropped. The documentation site sources, examples, tests, workflows, `Cargo.lock` and the changelog are not vendored; the licence files are. |
| `mongodb` | 3.10.0 | Let the TLS layer verify a hostname different from the one it dials. The SCRAM iteration cap and nonce prefix check are in the released crate since 3.10.0. |
| `redis` | 1.8.1 | Let a TLS connection verify a hostname different from the one it dials, so a certificate check survives an SSH tunnel's loopback dial address. Benchmarks, examples, tests, the changelog and `release.toml` are not vendored; the licence file is. |

A released dependency version with these behaviors would let the local copies be dropped; see
[dependency patch guide](../docs/vendor-patches.md).
