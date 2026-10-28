# Peer dependency review, October 28

Dated source review of the Rust packages that peer database clients use, compared with BookiE's vendored and direct dependencies. It records what was read and what it suggests. It selects no work: the [local dependency patch proposal](../proposals/upstream-patches.md) owns removal criteria, and the [backlog](../backlog.md) owns order.

## Evidence and limits

- BookiE: `d1fcc0285`, the GitHub `linux` tip.
- Peers, default branch read on 2026-10-28 (tips checked afterwards, so a few later commits may not be in the manifests): TablePro `86651d743`, dbx `3699fa053`, dbflux `607487160`, navop `cc25d7687`, fluxDB `9c9b1d073`, dbdelve `85114cee3`, rainfrog `ecc77e73d`, gobang `7b1b5f7eb`.
- Only `Cargo.toml` files were read, at most 40 per repository, with `vendor/`, `examples/`, `fixtures/` and test paths skipped. Version requirements are declared ranges, not locked versions. No peer source was run or copied.
- Crates.io versions were checked on the same day.

## Findings

| Topic | What peers do | BookiE today |
|---|---|---|
| Driver patches | None patch a database driver for security. Their `[patch]` sections vendor UI and platform crates (`gpui`, `wry`, `tauri`). dbx also forks `tokio-postgres` for GaussDB | Four vendored crates (`sqlx-postgres`, `sqlx-mysql`, `mongodb`, `redis`), each a pristine release plus one patch |
| PostgreSQL | Six of eight use `tokio-postgres`. TablePro declares `postgres-protocol 0.6.12` as a floor, which carries the SCRAM iteration cap | `sqlx` 0.9 with the cap added by a vendor patch |
| MySQL | dbx and navop use `mysql_async`; dbflux and dbdelve use `mysql` | `sqlx-mysql` 0.9 with two vendor patches |
| SQLite | Five use `rusqlite`, up to 0.40 | `sqlx` with the system SQLite (`libsqlite3-sys` bounded below 0.38; 0.39.1 is current) |
| DuckDB | dbdelve pins `=1.20000.0`; dbflux uses 1.10506 with Arrow 59; TablePro uses `~1.10506` | `duckdb` 1.10506.1 with Arrow 58 |
| `zip` | TablePro 8.3, dbx 4.x, dbflux and fluxDB 8 | 9.1, after the October 28 refresh |
| `rmcp` | TablePro 3.6, dbx 3.5.1 | 3.8 |
| `tiberius` | TablePro 0.13; dbx, dbflux, navop, dbdelve 0.12 | 0.13 |
| Not used by BookiE | `tree-sitter` (four peers), `aes` with `cbc` (four to five), the `oracle` crate (navop, rainfrog), `scylla` (TablePro) | Features or engines, not defect fixes |

## Observations

- Peers take driver fixes by choosing releases. BookiE's patches exist because the fixes were not released: the SQLx and MongoDB reports are tracked in the [proposal](../proposals/upstream-patches.md).
- DuckDB 2.0 is in use elsewhere. dbdelve's history contains "link the bundled extensions on Linux with GNU ld", a Linux build problem to check before BookiE moves to `1.20000`.
- BookiE links the system SQLite, so it cannot take a newer bundled SQLite the way `rusqlite` users can. That belongs to the SQLite runtime decision in [#1545](https://github.com/cozyGarage/BookiE/issues/1545).
- The connection URL parser in `crates/storage/src/connection_url.rs` is hand-written and tested. The `url` crate is already in the dependency tree, so no peer package is needed for it.
