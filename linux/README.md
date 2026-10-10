# BookiE for Linux

This is the canonical product, installation and build guide for the Linux
application. The repository overview is at [the root README](../README.md).

Current delivery scope and evidence: [active sprint](docs/bookie-0.2-sprint.md). The [October 3 consistency review](docs/archive/architecture-consistency-review-2026-10-03.md) reconciles the accumulated documentation. Earlier audits prove their recorded source trees; package release approval remains separate.

BookiE is a Linux-only database client built with Rust, GTK4, libadwaita, and Relm4. The Rust workspace is rooted in this `linux/` directory. The product was previously called TablePro on this branch; the rename covers the display name, binaries (`bookie`, `bookie-agentd`) and branding only. App ID, XDG paths, keyring schema, UUIDs, audit format and protocol contracts stay `tablepro` / `com.tablepro.linux`. Crate names remain `tablepro-*`.

## Status

The GTK application supports PostgreSQL, MySQL, SQLite, SQL Server, and ClickHouse. Redis and MongoDB are experimental; Redis 0.2.0 support uses one host/port endpoint, with Sentinel and Cluster deferred. DuckDB is an optional build feature.

Current workflows include saved connections, SSH tunnels, browse and SQL tabs, structure editing, inline row changes, query history, policy checks, MCP access, and the headless `bookie-agentd` process. See [ROADMAP.md](ROADMAP.md), [docs/connections.md](docs/connections.md), [docs/adding-drivers.md#driver-maturity](docs/adding-drivers.md#driver-maturity), and [docs/production-audit.md](docs/archive/production-audit.md) for current limits.

The Linux client remains under development toward 0.2.0. B3 type/value consumer coverage is still expanding; B4 and installed desktop qualification remain open. The [active sprint](docs/bookie-0.2-sprint.md) records current implementation and acceptance. Installable 0.1.5 packages are on the [linux-v0.1.5 GitHub Release](https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.1.5); they do not qualify the 0.2.0 source. Wayland soak remains an operator check after install.

## Linux versions

BookiE 0.1.5 (tag `linux-v0.1.5`) targets:

| Distro | Arch | Artifact | Package name | Launch |
|---|---|---|---|---|
| Debian / Ubuntu with the required GNOME 50 libraries | amd64 | `tablepro_0.1.5-1_amd64.deb` | `tablepro` | `bookie` |
| Arch Linux / Omarchy | x86_64 | `bookie-0.1.5-1-x86_64.pkg.tar.zst` | `bookie` | `bookie` |

`tablepro` and `tablepro-agentd` remain command aliases. Application ID and XDG paths stay `com.tablepro.linux` / `tablepro`.

**Ubuntu 24.04 and 25.10 cannot run this line.** GTK CI builds in Debian testing for the GNOME 50 library baseline.

Runtime baseline: GTK4 4.14+, GLib 2.80+, libadwaita 1.5+, GtkSourceView 5.12+ (Ubuntu 24.04 and Debian 13 or newer). Build with Rust 1.98. No AUR, Flathub, or Fedora package. No 32-bit or ARM artifacts.

## Named query parameters

Write `:name` anywhere a value belongs:

```sql
SELECT * FROM orders WHERE customer = :customer AND total > :minimum;
```

Running the statement asks for one value per name and sends them as driver-bound parameters. Each value has a type choice: `Auto` (whole numbers and decimals are detected, everything else is text), `Text`, `Integer`, `Decimal`, `Boolean`, or `Null`. Placeholders inside string literals, quoted identifiers, comments, dollar-quoted bodies, PostgreSQL `::` casts, and existing `$1` or `?` placeholders are left alone.

## Editor productivity

- **Completion**: typing after `FROM` or `JOIN` offers tables; elsewhere it offers the columns of the tables named in the statement. `alias.` and `table.` narrow to that table's columns, and columns are fetched on demand for tables you reference.
- **Favorites**: Ctrl+D saves the current editor query under a name in `$XDG_CONFIG_HOME/tablepro/favorites.json`. Saving an existing name replaces its statement.
- **Open Quickly**: Ctrl+P searches favorites, open tabs, and saved connections. Type to filter (name, statement text, or initials), arrow keys to move, Enter to open, Escape to dismiss.

## Stack

| Layer | Technology |
|---|---|
| Language | Rust 1.98+ |
| GUI | GTK4 4.14+, GLib 2.80+, libadwaita 1.5+, GtkSourceView 5.12+ |
| Components | Relm4 |
| Async work | Tokio for database and service work, GLib main context for GTK |
| Drivers | SQLx for PostgreSQL, MySQL, and SQLite; Tiberius for SQL Server; engine-specific crates for ClickHouse, Redis, MongoDB, and DuckDB |
| Decimal values | `rust_decimal` carrier with precision/scale validation at each supported consumer |
| Storage | XDG JSON files, SQLite FTS5, JSONL audit journal, Secret Service through `oo7` |
| Packaging | GitHub Release `.deb` (required native libraries below) and Arch `.pkg`; no AUR or Flathub yet |

Drivers are linked at build time. BookiE does not load database drivers as runtime plugins. The UI uses native GTK widgets and does not embed a browser view.

Table browsing fetches bounded pages from the database, and GTK creates row objects on demand for the loaded page. Arbitrary SQL editor results are still materialized up to configured caps; progressive server-cursor paging is accepted in [ADR 0011](docs/decisions/0011-paged-query-results.md) but not implemented. Row-object reuse does not bound the memory held by a query result.

## Build requirements

```bash
# Ubuntu / Debian
sudo apt install -y build-essential pkg-config libgtk-4-dev libadwaita-1-dev \
  libgtksourceview-5-dev libssl-dev libsecret-1-dev libkrb5-dev libsqlite3-dev clang

# Fedora (build from source only; no 0.1.5 package)
sudo dnf install -y gcc pkg-config gtk4-devel libadwaita-devel \
  gtksourceview5-devel openssl-devel libsecret-devel krb5-devel clang

# Arch
sudo pacman -S --needed base-devel pkg-config gtk4 libadwaita \
  gtksourceview5 openssl libsecret krb5 sqlite clang
```

Check the native libraries and Rust toolchain:

```bash
pkg-config --modversion gtk4 libadwaita-1 gtksourceview-5
rustc --version
```

## Build and run

Run Cargo commands from the workspace root:

```bash
cd linux
cargo run -p tablepro-app
```

A plain `cargo run` shares settings, saved connections, the keyring entries and the single-instance lock with an installed BookiE. To keep test data apart, build with the development profile, which uses `~/.config/tablepro-devel` and its own keyring schema:

```bash
TABLEPRO_PROFILE=development cargo run -p tablepro-app
```

Starting a second copy of the same profile asks the running one to show its window and then exits.

### SQL Server Kerberos

Run `kinit` before connecting and confirm the ticket with `klist`. Select **Windows (Kerberos)** in the SQL Server connection form and enter the server's real DNS hostname. SQL Server requests `MSSQLSvc/<host>:<port>`, including when SSH forwards the socket through localhost.

A FILE credential cache works when `KRB5CCNAME` points to a readable location. Custom SPN overrides and cross-realm setup are not exposed in the connection form.

## Checks

From `linux/`:

```bash
./scripts/preflight.sh
```

`preflight.sh` runs size and panic guards, formatting, Clippy, unit tests and the sandbox tier. It does not run the installed GTK suite. Pick further layers from the [validation playbook](docs/validation-playbook.md) (for example `./scripts/ci-local.sh integration` for Docker driver suites, or `./scripts/test-gtk-widgets.sh` when GTK widgets change).

**CI tiers:** GitHub pull requests and pushes to `linux` / `main` run the cheap tier only. A green GitHub check is not merge acceptance. The merge tier (Docker drivers, installed GTK, distro floor, packages, driver TLS, PostgreSQL release fixture, DuckDB) runs on the lab Forgejo on every branch push. Gate with `bash linux/scripts/forgejo-gate.sh <branch>` from the repository root ([AGENTS.md](../AGENTS.md)).

To smoke-test a PostgreSQL server you already run:

```bash
./scripts/smoke-postgres.sh
```

For manual connection checks across every network driver, use the local-only [manual connection fixture](tests/manual-connections/README.md).

To run the disposable real Unix-socket fixture:

```bash
./scripts/test-postgres-socket.sh
```

If native development packages are unavailable, `scripts/dev-env.sh` can use Debian-family package payloads extracted under `../.local-deps/root/`.

## Packaging

The current package is BookiE 0.1.5. Download it from the [linux-v0.1.5 GitHub Release](https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.1.5), or see [the release evidence](docs/archive/release-0.1.4.md) and [packaging/README.md](packaging/README.md). To rebuild the Arch package from a clean commit:

```bash
TABLEPRO_RC_COMMIT="$(git rev-parse HEAD)" TABLEPRO_RC_VERSION=0.1.5 ./scripts/build-arch-rc.sh
```

The helper archives that commit, verifies a real checksum, and does not publish to AUR. Set `TABLEPRO_RC_TAG=linux-v…` instead of `TABLEPRO_RC_COMMIT` only to verify an already published tag.

## Documentation

Start with [the documentation entry point](docs/README.md), then the active sprint and relevant ADR. Use the value evidence index to read a case instead of loading the full historical ledger.

| Topic | File |
|---|---|
| Architecture and crate boundaries | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Roadmap | [ROADMAP.md](ROADMAP.md) |
| User guide | [docs/user-guide.md](docs/user-guide.md) |
| Feature comparison with other clients | [docs/0.2-feature-comparison.md](docs/0.2-feature-comparison.md) |
| Production audit (historical) | [docs/archive/production-audit.md](docs/archive/production-audit.md) |
| Contributing | [CONTRIBUTING.md](CONTRIBUTING.md) |
| Supported distros, Rust toolchain, Arch package, Flathub, accessibility | [docs/platforms.md](docs/platforms.md) |
| Historical September stabilization evidence | [docs/archive/stabilization-2026-09.md](docs/archive/stabilization-2026-09.md) |
| Ignored-test inventory | [docs/ignored-tests.md](docs/ignored-tests.md) |
| Whole-app macOS 0.72 gaps (historical) | [docs/archive/upstream-adoption.md](docs/archive/upstream-adoption.md) |
| Optional upstream reference review | [docs/upstream-sync.md](docs/upstream-sync.md) |
| Adding a database driver | [docs/adding-drivers.md](docs/adding-drivers.md) |
| Driver maturity | [docs/adding-drivers.md#driver-maturity](docs/adding-drivers.md#driver-maturity) |
| State management | [docs/state-management.md](docs/state-management.md) |
| Connection handling | [docs/connections.md](docs/connections.md) |
| Capability evidence | [docs/capability-evidence.md](docs/capability-evidence.md) |
| Storage | [docs/storage.md](docs/storage.md) |
| Error handling | [docs/error-handling.md](docs/error-handling.md) |
| Testing | [docs/testing.md](docs/testing.md) |
| Regression audit 2026-09-29 (historical) | [docs/archive/regression-audit-2026-09-29.md](docs/archive/regression-audit-2026-09-29.md) |
| Architecture decisions | [docs/decisions/](docs/decisions/) |

## License

BookiE is licensed under AGPL-3.0-or-later. See [LICENSE.md](LICENSE.md).
