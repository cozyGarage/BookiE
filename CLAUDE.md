# CLAUDE.md

This file defines the repository rules for coding agents and contributors.

## Project scope

TablePro is a Linux-only native database client. Current development stays on the `linux` branch. The source is a Rust 1.98 Cargo workspace under `linux/`.

The Linux app is being renamed to **BookiE** (`linux/README.md`, `linux/ROADMAP.md`). The active plan is [`linux/docs/bookie-0.2-sprint.md`](linux/docs/bookie-0.2-sprint.md): continue from the integrated BookiE 0.1.4 baseline toward 0.2.0; the original 0.1.1 milestone is historical. The rename covers display name, binary names (`bookie`, `bookie-agentd`), and branding assets only — app ID, config/data paths, keyring schema, UUIDs, audit format, and protocol contracts stay `tablepro`/`com.tablepro.linux` for compatibility. Do not rename those identifiers without checking that plan first.

The UI uses GTK4, libadwaita, GtkSourceView, and Relm4. Database drivers are static workspace crates linked into the app. Keep driver registration at compile time. Do not add cross-platform UI layers, web views, or source for another operating system.

Every shipped feature must be available without an account, license key, subscription, paid tier, or remote entitlement check.

The root [LICENSE](LICENSE) applies to the repository. User-facing changes are recorded in [linux/CHANGELOG.md](linux/CHANGELOG.md).

## Principles

1. Security comes first. Validate input at system boundaries and deny unsafe operations by default.
2. Fix root causes. Reproduce or trace a defect before changing code.
3. Keep dependencies one-directional and preserve crate boundaries.
4. Use clear names, small functions, early returns, and explicit error paths.
5. Do not add comments. Code, types, tests, and module boundaries must express intent. Record only what an external system forces on us and cannot be named.
6. Every testable behavior change needs a regression test.
7. Keep changes focused. Do not mix unrelated cleanup into a fix.
8. Do not add feature gates based on accounts, licenses, subscriptions, payment, or remote access checks.

## Workspace architecture

The workspace manifest is `linux/Cargo.toml`.

- `linux/crates/core`: domain types, driver traits, query results, filters, transactions, and the driver registry. It has no dependency on another workspace crate.
- `linux/crates/policy`: statement classification, rules, approvals, masking, blast-radius checks, and audit types. It depends on `core` only.
- `linux/crates/storage`: saved connections, Secret Service access, query history, and the audit journal.
- `linux/crates/ssh`: SSH tunnels through `russh`.
- `linux/crates/transport`: connection assembly. Turns a saved connection into driver options, resolves its SSH chain, opens the tunnel, and preserves the service endpoint used for certificate verification. The GUI and `agentd` both connect through it.
- `linux/crates/mcp`: MCP authentication, scopes, connection allowlists, rate limits, tools, and transport.
- `linux/crates/agentd`: headless MCP process and composition root without GTK.
- `linux/crates/drivers/*`: one static crate per database engine. Driver crates implement `core` traits and do not depend on the app.
- `linux/crates/app`: GTK4/libadwaita application and composition root. Relm4 components own UI state and route async results back to the GTK main context.

Keep dependencies pointed toward `core`. `app` and `agentd` assemble concrete drivers and services. Domain and driver crates must not import GTK or Relm4.

Add a database engine as a workspace driver crate, implement the `tablepro-core` contracts, add it to the composition roots, document its maturity, and test it against a real engine. Drivers are compiled into the binaries.

## UI and async rules

GTK widgets belong to the glib main context. Database and blocking work must not run on the GTK thread.

Use Relm4 component messages for state transitions. Use component-scoped commands for work whose lifetime belongs to a component. Return async outcomes to the component update loop before touching widgets. Keep reusable logic outside widget construction so it can be unit-tested.

Cancellation and timeouts must reach the database operation. A dropped UI future is not proof that a driver stopped. Late results must not replace state from a newer connection or query attempt.

## Security invariants

These rules apply to the GUI, MCP server, and `tablepro-agentd`.

- Every database connection exposed to a consumer must be wrapped by `PolicyGuard`. Do not expose a raw driver connection from MCP or agent code.
- MCP token scopes establish who may call a tool. Connection allowlists establish which saved connections a token may use. `PolicyGuard` establishes what SQL may run. All three checks are required.
- Preview, transaction, retry, and batch paths must pass through the same policy checks as direct execution.
- Statement handling must preserve the order classify, evaluate rules, request approval when required, apply masking, execute, and write the audit outcome.
- Denied, failed, cancelled, and timed-out operations must produce the required terminal audit state. Audit failure must never open a path around policy.
- Treat MCP input, saved connection files, imported files, environment variables, and database metadata as untrusted input.
- Bind data values through driver parameters. Validate and dialect-quote identifiers. Never build SQL by joining untrusted text.
- Keep passwords, tokens, and SSH secrets in Secret Service through `tablepro-storage`. Do not write secrets to JSON, command lines, traces, errors, or audit fields.
- Keep secret values wrapped in `secrecy` types until they reach the driver boundary.
- Apply bounded request sizes, query limits, timeouts, and rate limits at external interfaces. Avoid unbounded queues and collections controlled by callers.
- MCP tools must use least-privilege scopes and deny access when a token, scope, allowlist entry, or policy decision is missing.
- Dependency additions require a license and advisory review with `linux/deny.toml`.

## Rust code style

`linux/rustfmt.toml`, `linux/clippy.toml`, and the workspace lints are authoritative.
[`linux/docs/code-conventions.md`](linux/docs/code-conventions.md) settles the recurring shape
questions this file does not: function length, parameter count, naming, when to extract shared
code, and the decisions that must not be reversed in passing.

- Use Rust edition 2024 and Rust 1.98.
- Format with `rustfmt`; the line width is 120 characters.
- Do not add comments, including documentation comments. Prefer clear module, type, function, and test names. The single exception is behaviour of something outside this repository that no name can express: a dependency's parser, an engine's SQL quirk, a platform or protocol contract. Such a comment states the external rule and what breaks without it, never what the code does. A comment explaining our own code is still a signal to rename or restructure it.
- Use early returns to keep control flow flat. Three levels of indentation inside a function body is the limit before extracting.
- Keep a function body at or below 60 lines. `linux/scripts/check-function-size.py` enforces this and freezes existing longer functions per file in `linux/function-size-baselines.txt`. Lower a count when you split a function; never raise one.
- Keep public APIs small. Default to private visibility.
- Do not use `unwrap`, `expect`, `panic!`, `todo!`, or `unimplemented!` in production paths. The workspace lints deny these, so a violation fails Clippy rather than review. Unit tests in a `#[cfg(test)]` module are exempt through `linux/clippy.toml`; a new integration test file under a crate's `tests/` directory must start with `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`, because those settings do not reach separate test crates.
- Use typed `thiserror` errors across crate boundaries. Add context to internal failures without exposing secrets.
- Avoid `unsafe`. If a native API makes it unavoidable, isolate it behind the smallest safe interface and require focused tests and review.
- Do not suppress Clippy lints to avoid fixing code unless the lint is wrong for a documented repository-wide reason.
- Use `tracing` fields for application logs. The workspace lints deny `print!`, `println!`, `eprint!`, `eprintln!` and `dbg!`. Protocol output on stdout must remain separate from logs and writes through an explicit `std::io::stdout` handle so a failed write is reported rather than swallowed.
- Do not log SQL parameters, credentials, tokens, connection strings, or unmasked query results.

## Tests

Put unit tests near pure logic and integration tests in each crate's `tests/` directory. Driver behavior must be tested against a real database through testcontainers or an isolated local test database. Use `tempfile` for filesystem tests.

Policy or MCP changes must test denied and allowed cases. They must also test scopes, connection allowlists, approval behavior, masking, timeouts, and audit terminal states when affected.

UI logic should be extracted into testable services. For GTK-only behavior that cannot run reliably in automation, describe manual steps and include light and dark screenshots in the pull request.

Never change a test to accept incorrect behavior. Fix the implementation or correct an invalid expectation with a clear reason.

### Regression tiers

Every test belongs to exactly one tier, and every tier has one script and one gate.

| Tier | Contents | Script | Gate |
|---|---|---|---|
| unit | `--lib --bins` across the workspace | `linux/scripts/preflight.sh` | CI `preflight` and `fast` |
| sandbox | Integration targets needing no Docker, no database service, and no display | `linux/scripts/test-sandbox.sh` | CI `preflight` |
| driver | `crates/drivers/*/tests/integration.rs` against a container | none | CI `integration` |
| driver-tls | `crates/driver-tls-tests` against network servers holding a privately issued certificate | `linux/scripts/test-driver-tls.sh` | CI `driver-tls` |
| release | `crates/release-tests` against the PostgreSQL fixture | `linux/scripts/test-postgres-release.sh` | CI `postgres-release` |
| gtk-widgets | `#[ignore]` widget tests needing an isolated display, listed in `linux/scripts/isolated-tests.json` under `gtk` | `linux/scripts/test-gtk-widgets.sh` | CI `fast` |
| keyring | `#[ignore]` tests needing a live Secret Service, listed in `linux/scripts/isolated-tests.json` under `keyring` | `linux/scripts/test-secret-service.sh` | CI `gtk-safety` |
| gtk | `crates/app/tests/gtk_safety.py` on an installed build | `linux/scripts/test-gtk-safety.sh` | CI `gtk-safety` |

The sandbox script selects targets with `--tests` rather than by name, so a new integration file is gated as soon as it is added. Adding a crate to the workspace means adding it to the crate lists in `preflight.sh` and `test-sandbox.sh`.

The gtk-widgets and keyring tiers are selected by name, not by pattern, so a test added to either must also be added to `isolated-tests.json` or nothing runs it. Their runner asserts that each named test executed exactly once, so a renamed or deleted entry fails the tier instead of silently passing.

Every fixed defect gets a regression test in the lowest tier that can reproduce it, and the fix and its test land in the same commit. Before relying on a new regression test, confirm it fails against the unfixed code.

## Function-size guard

Run `python3 linux/scripts/check-function-size.py` for every Rust change.

- A function body may be at most 60 lines, counted between its braces.
- `linux/function-size-baselines.txt` records how many over-limit functions each existing file may still contain. A listed file may not gain another.
- Lower a count in the same commit that splits a function. Do not raise one.
- `#[cfg(test)]` modules and files under a crate's `tests/` are excluded.
- GTK widget construction and Relm4 message dispatch are the known exception, already baselined. Logic inside them still moves to a testable free function.

## File-size guard

Run `linux/scripts/check-file-size.sh` for every Rust change.

- New Rust files should stay at or below 1,200 lines.
- Unlisted Rust files above 1,800 lines fail validation.
- Files in `linux/file-size-baselines.txt` may not grow beyond their listed ceiling.
- Split modules by responsibility before raising a baseline. If a listed file shrinks, lower its baseline in the same change.

## Changelog

Follow Keep a Changelog 1.1.0 in `linux/CHANGELOG.md`. Add user-facing changes under `[Unreleased]` in `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`.

Each entry is one line and describes user impact. Do not include file paths, type names, or function names. Do not add a `Fixed` entry for a defect introduced and corrected before release. Fold that correction into the unreleased `Added` or `Changed` entry. Documentation-only changes do not need an entry.

## Commits

Use Conventional Commits 1.0.0 with a single-line subject and no body:

```text
feat(app): add saved query tabs
fix(policy): deny writes after approval timeout
refactor(drivers): share SQL value conversion
security(mcp): reject tokens outside the connection allowlist
```

Allowed types are `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `chore`, `style`, `revert`, and `security`. Prefer scopes such as `app`, `core`, `policy`, `mcp`, `agentd`, `storage`, `ssh`, `drivers`, or `driver-postgres`.

Public API changes must update every caller and test in the same commit.

## Validation

Run commands from the repository root. Start with the narrow test for the changed crate, then run the full checks that apply.

```bash
bash linux/scripts/check-file-size.sh
bash linux/scripts/check-panic-sites.sh
python3 linux/scripts/check-function-size.py
bash linux/scripts/check-bounded-operations.sh
cargo fmt --manifest-path linux/Cargo.toml --all -- --check
cargo clippy --manifest-path linux/Cargo.toml --workspace --exclude tablepro-driver-duckdb --all-targets -- -D warnings
cargo test --manifest-path linux/Cargo.toml --workspace --exclude tablepro-driver-duckdb --lib --bins
cargo test --manifest-path linux/Cargo.toml -p tablepro-mcp --test enforce_policy
cargo test --manifest-path linux/Cargo.toml -p tablepro-mcp --test timeout_audit
cargo deny check   # run from linux/; it does not accept --manifest-path
```

Driver integration tests require Docker or a compatible Podman socket:

```bash
cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-postgres --test integration -- --include-ignored --test-threads=1
cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mysql --test integration -- --include-ignored --test-threads=1
cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mssql --test integration -- --include-ignored --test-threads=1
cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-clickhouse --test integration -- --include-ignored --test-threads=1
cargo test --manifest-path linux/Cargo.toml -p tablepro-driver-mongodb --test integration -- --include-ignored --test-threads=1
```

`linux/scripts/preflight.sh` runs the file-size, function-size, panic-site and bounded-operation guards, formatting, Clippy, the unit tier and the sandbox tier in one pass, and is the quicker way to cover most of the above.

If required GTK development packages, database services, containers, or `cargo-deny` are unavailable, report which validation could not run and why.

## Writing style

Use short, plain sentences. Be specific. Do not use em dashes. Avoid sales language and generic praise. User-facing text must describe behavior and next steps without exposing internal errors or secrets.
