# TablePro repository

This repository contains the Linux BookiE application, derived from TablePro.
This root page describes the repository and its development workflow. For
BookiE features, supported systems, installation and user-facing guidance, use
the canonical [Linux application guide](linux/README.md). The two pages have
different audiences and should not duplicate product or build instructions.

Current delivery scope and evidence: [active sprint](linux/docs/bookie-0.2-sprint.md). The [October 3 consistency review](linux/docs/archive/architecture-consistency-review-2026-10-03.md) reconciles the accumulated documentation. Earlier audits prove their recorded source trees; package release approval remains separate.

TablePro is a native Linux database client built with Rust, GTK4, libadwaita, GtkSourceView, and Relm4. Current development is on the `linux` branch. The Cargo workspace is under `linux/` and requires Rust 1.98.

Every shipped feature is free to use. TablePro has no account, license, subscription, paid-tier, or remote entitlement gate.

## Status

The Linux client is under active development toward BookiE 0.2.0. B3 type/value consumer coverage remains open, followed by B4 transport/session acceptance and installed desktop qualification. It includes database browsing, SQL editing, structure editing, inline row changes, query history, SSH tunnels, policy checks, audit records, MCP access, and a headless MCP process. The GTK grid creates row objects on demand over an in-memory result; it does not yet stream or page large results from the database.

PostgreSQL is the furthest along: server-confirmed cancellation, certificate hostname and authority verification, verified TLS through an SSH tunnel, read-only denial, rollback, blocking-lock reporting, and reconnect run as deterministic container checks. MySQL, ClickHouse, Redis, and MongoDB also have driver TLS fixture evidence; those fixtures do not establish full transport or packaging readiness. Packaging is not release-verified. [`linux/ROADMAP.md`](linux/ROADMAP.md) links to current milestone status and the detailed acceptance boards.

Database support is provided by static Rust crates compiled into the app:

| Database | Status |
|---|---|
| PostgreSQL | Implemented; PostgreSQL release fixture |
| MySQL and MariaDB | Implemented; driver and TLS fixtures |
| SQLite | Implemented; local and GTK tests |
| Microsoft SQL Server | Implemented; cancellation and authentication limits |
| ClickHouse | Implemented; driver and TLS fixtures |
| Redis | Experimental |
| MongoDB | Experimental |
| DuckDB | Optional build feature |

See [`linux/docs/adding-drivers.md`](linux/docs/adding-drivers.md#driver-maturity) for current limits.

See the [bug and consistency audit](linux/docs/archive/bug-consistency-2026-09.md) for its dated test results and the [macOS 0.72 gap review](linux/docs/archive/upstream-adoption.md) for follow-up features.

## Architecture

- `linux/crates/app`: GTK4/libadwaita application and Relm4 components
- `linux/crates/core`: domain types and database driver contracts
- `linux/crates/drivers/*`: static database driver crates
- `linux/crates/policy`: SQL classification, approvals, masking, and audit types
- `linux/crates/mcp`: MCP authentication, scopes, allowlists, rate limits, and tools
- `linux/crates/agentd`: headless MCP process
- `linux/crates/storage`: Secret Service integration, saved connections, history, and audit journal
- `linux/crates/transport`: shared saved-connection, credential, and route assembly for GUI and daemon
- `linux/crates/ssh`: SSH tunnels

All GUI, MCP, and agent database access passes through policy-gated connection handles. MCP scopes and connection allowlists do not replace SQL policy checks.

Read [`linux/ARCHITECTURE.md`](linux/ARCHITECTURE.md) for crate boundaries and data flow.

## Build

Install Rust 1.98 and the GTK development packages listed in [`linux/README.md`](linux/README.md), then run from the repository root:

```bash
cargo run --manifest-path linux/Cargo.toml -p tablepro-app
```

Optional drivers:

```bash
cargo run --manifest-path linux/Cargo.toml -p tablepro-app --features duckdb
```

## Validate

```bash
bash linux/scripts/check-file-size.sh
cargo fmt --manifest-path linux/Cargo.toml --all -- --check
cargo clippy --manifest-path linux/Cargo.toml --workspace --exclude tablepro-driver-duckdb --all-targets -- -D warnings
cargo test --manifest-path linux/Cargo.toml --workspace --exclude tablepro-driver-duckdb --lib --bins
```

Deeper gates, run from `linux/`:

```bash
./scripts/ci-local.sh integration          # container driver tests
./scripts/test-postgres-release.sh         # PostgreSQL TLS, SSH, lock, and reconnect fixture
./scripts/test-gtk-safety.sh               # installed GTK approval and audit flows
./scripts/ci-local.sh release              # all three in sequence
```

Container-backed driver tests and more setup details are in [`linux/docs/testing.md`](linux/docs/testing.md).

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md). Pull requests for current development target the `linux` branch.

## Changelog

Release notes and unreleased changes are in [`linux/CHANGELOG.md`](linux/CHANGELOG.md).

## License

TablePro is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE).
