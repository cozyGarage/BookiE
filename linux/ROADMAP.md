# BookiE roadmap

Active delivery plan: [BookiE 0.1.x → 0.2 sprint](docs/bookie-0.2-sprint.md),
approved September 16, refreshed against 0.1.4 on September 17. It supersedes older sequencing below.
See the [30-commit baseline review](docs/baseline-review-2026-09-17.md) before further convergence work.

Current scope and verification are recorded in the [approved sprint ledger](docs/bookie-0.2-sprint.md). Prior audits retain evidence for their own source trees.

The repository-level [`PLAN.md`](../PLAN.md) is the source of truth for sequencing, detailed acceptance criteria, and the Linux capability backlog. This file is the concise status view.

## Current state

BookiE is a GTK4/libadwaita database client. Its core daily-driver workflows are implemented. Production approval, fail-closed audit, PostgreSQL cancellation, and PostgreSQL TLS, SSH, lock, and reconnect behavior are release-verified locally. Exclusive connection switching, deterministic PostgreSQL ordering, direct local sockets, the internal Arch recipe, and a required GTK job are implemented. A release candidate must be frozen before its hosted, soak, and installed-package evidence can be attributed to it.

Every claim below states whether it is implemented, integrated, or release-verified. A feature with unit tests only is never described as verified.

Status terms:

- **Implemented**: code and core unit tests exist.
- **Integrated**: all intended production entry points use it.
- **Release-verified**: deterministic real-driver, UI, or installed-package tests prove it.
- **Complete**: all phase acceptance criteria are release-verified.
- **Partial**: release-verified on some drivers or paths and unproven or absent on others.

## Verified inventory

| Area | Status | Notes |
|---|---|---|
| PostgreSQL, MySQL, SQLite, SQL Server, ClickHouse | Implemented | PostgreSQL is release-verified through the fixture; the other engines have container integration tests only. Server-side cancellation is verified against a real engine on PostgreSQL, MySQL, ClickHouse and SQLite; SQL Server declares it unsupported because tiberius cannot send the TDS attention packet |
| Redis, MongoDB, DuckDB | Implemented | Experimental; DuckDB requires a Cargo feature. Redis and MongoDB TLS is release-verified |
| Browse/edit/filter/sort/pagination | Implemented | Keyset helper exists; integers wider than 2^53 edit exactly; large-result behavior needs release tests |
| SQL editor and multiple result tabs | Integrated | PostgreSQL timeout and cancel stop the server query and wait for terminal audit state. One dialect-aware lexer sets statement boundaries, so a PostgreSQL function body runs whole. A lone BEGIN/COMMIT/ROLLBACK is refused on the shared connection; an opt-in per-tab Session (PostgreSQL, MySQL, SQL Server, SQLite files) keeps settings, temp tables and transactions, governed and audited. SQL files open, save and relink after restart, with a changed-on-disk check; GTK rendering unverified |
| Bounded operations | Integrated | Every database call the interface starts carries a deadline, gated by `scripts/check-bounded-operations.sh` |
| Structure editor | Implemented | Tables, columns, indexes, foreign keys and column comments; expression and partial PostgreSQL indexes are shown read-only; a column collation is kept on PostgreSQL, MySQL and SQL Server alters |
| Saved connections and libsecret | Implemented | Keyring failure UX needs hardening |
| SSH and jump chains | Integrated | A verifying PostgreSQL connection forwards through a private Unix socket and is release-verified, headlessly as well as in the GUI; jump chains are JSON-only in the current GTK form. An optional system OpenSSH client (forced host-key prompts, per-host secret binding, ProxyJump from ssh_config) is implemented and tested against a real sshd, not release-verified. agentd refuses unknown host keys |
| TLS modes | Partial | Release-verified on PostgreSQL, including `VerifyFull` through SSH. Release-verified on MySQL, ClickHouse, MongoDB, and Redis through the driver TLS fixture. Mapped but untested on SQL Server; custom certificate authorities are implemented but their real-server verification remains unproven. Saved connections carry a certificate authority. See [docs/connections.md](docs/connections.md) |
| Query history | Implemented | MCP access must be isolated before being re-exposed |
| Export and import | Implemented | Loaded results export as CSV, JSON, Markdown, HTML, XML, SQL INSERT or Excel; CSV imports into a new or existing table under one scoped approval. Full-table snapshot streaming and Parquet are deferred |
| Activity and EXPLAIN | Implemented | Administrative classification and numeric session-ID validation are covered |
| Policy, MCP, and agentd | Integrated | Approval and audit failures deny governed operations; a policy file that cannot be read keeps the last good policy and leaves MCP off; `list_tables` and `describe_table` use the same timeout and identifier checks as the other metadata tools; read-only denial is release-verified against PostgreSQL; the GUI and agentd share one connection transport, release-verified through the fixture bastion |
| Audit journal | Integrated | Durable intent/outcome records, recovery, private mode, and cross-process locking are locally verified |
| Internal Arch RC | Implemented | Immutable-commit/checksum recipe exists; install, upgrade, rollback, and Wayland verification remain |
| Debian, Flatpak, AUR | Scaffolded | Not release targets and not ready for public publication |
| i18n and accessibility | Infrastructure | English strings/checklist exist; end-user verification is incomplete |

## Active phases

### 0: Development baseline

- [x] Reconcile upstream SQL Server Kerberos and service identity without losing fork safety work
- [x] Preserve legacy connection serialization
- [x] Pass Clippy on Rust 1.93 and current stable Rust
- [x] Document the Arch/rustup toolchain setup
- [x] Add scheduled current-stable CI
- [x] Reject untrusted browser origins on the MCP loopback endpoint
- [x] Detect SSH host-key changes across key algorithms
- [x] Pin Linux GitHub Actions to immutable commits
- [x] Log upstream reconciliations from 2026-08-10 onward

Phase 0 is complete locally. Real SQL Server TLS and Kerberos negotiation remain release-fixture work in Phase 3.

### 1: Authorization and approval

- [x] Replace production automatic approval with principal-aware routing
- [x] Enforce read-only before unparseable-statement fallback
- [x] Classify administrative and PostgreSQL side-effecting functions
- [x] Preserve DDL and unscoped DML restrictions across mixed transaction batches
- [x] Validate numeric session identifiers before building SQL
- [x] Require explicit saved-connection allowlists when agentd issues tokens
- [x] Remove or isolate MCP query-history search
- [x] Merge partial policies onto secure environment defaults

Phase 1 is implemented, security-reviewed, and locally verified. Phase 4 still owns release-level GTK proof for dialog dismissal and approve-once behavior.

### 2: Fail-closed audit

- [x] Disable governed writes and in-app MCP when audit initialization fails
- [x] Record durable intent before mutations and transaction completion
- [x] Record explicit, sanitized outcomes after operations
- [x] Verify journal mode, writability, legacy migration, recovery, and cross-process locking
- [x] Refuse agent service when audit storage is unavailable or unresolved writes exist
- [x] Persist unresolved write state across restarts and concurrent processes

Phase 2 is implemented, security-reviewed, and locally verified. PostgreSQL cancellation and terminal outcomes are now verified in Phase 3.

### 3: PostgreSQL release safety

- [x] Add the deterministic PostgreSQL TLS, SSH, lock, and reconnect fixture
- [x] Verify direct `VerifyFull`, wrong hostname, and unknown certificate authority
- [x] Prove a TCP-forwarded `VerifyFull` never verifies the local dial address
- [x] Implement and verify server-side cancellation, parameterized cancellation, rollback, and pool reuse
- [x] Verify batch and interactive rollback, activity, blocking locks, direct reconnect, and SSH reconnect
- [x] Connect a tunnelled `VerifyFull` session using the original database hostname

The fixture runs from `./scripts/test-postgres-release.sh` and in the `postgres-release` CI job. PostgreSQL tunnels a verifying connection over a private Unix socket, so the TCP dial path and the TLS server name are independent.

### 4: GTK safety tests

- [x] Dialog close and unexpected responses map to denial
- [x] Approve-once is not cached across policy operations
- [x] Disabled audit state takes precedence over an approving sink
- [x] Installed GTK dismissal leaves SQLite unchanged
- [x] Installed GTK approve-once prompts again for the next operation
- [x] Installed GTK audit failure cannot be approved around
- [x] Installed GTK pending row edits gate a connection switch without writing to the old database
- [x] Installed GTK browse tabs read the connection they were reopened against
- [x] Installed GTK connection switch keeps the previous connection's tabs after the persist debounce
- [x] Installed GTK pending edits stay in their own window when another window switches

Phase 4 has a required PR smoke job and a separate daily five-attempt soak. Promotion still requires 30 consecutive retry-free attempts across at least six workflow runs, all at one pinned commit. Both workflows now take an explicit `ref` and record the commit they resolved; before that the scheduled soak followed the branch tip, so attempts could not be attributed to a candidate. Adding a scenario restarts the ledger. The ledger stands at 0 of 30.

### 5: Documentation and capability tracking

- [x] Replace the historical roadmap with evidence-based status
- [x] Rewrite repository guidance for the Linux Rust and GTK application
- [x] Synchronize the production audit with server-side PostgreSQL cancellation evidence
- [x] Replace source-parity tracking with a Linux capability backlog
- [x] Keep external planning research advisory rather than authoritative
- [x] Distinguish implementation from release verification in every product claim

Phase 5 documentation is maintained as part of each change, not as a one-time task.

### 6: DBA and data-engineering depth

- [x] Named query parameters bound by the driver, release-verified against PostgreSQL and in the installed GTK suite
- [x] Schema-aware editor completion, saved favorites, and Open Quickly, release-verified in the installed GTK suite
- [x] SQL file open/save with external-change detection (implemented; GTK unverified)
- [ ] PostgreSQL objects, users/roles, and administration
- [ ] Import/export and backup/restore
- [x] Connection groups, tags, favourites, search, URL import, and environment colour (Phase 10.2)
- [ ] Reusable SSH and transport profiles
- [ ] True result streaming and optional Parquet export

### 7: Driver expansion

Prioritize real workflows: Redshift and CockroachDB profiles, Trino, Snowflake, then BigQuery. Other drivers remain demand-driven.

### 8: Identity and packaging

Promote the internal Arch package only after its installed-package and soak
gates pass. Finalize the product name, repository, and application ID before
any later AUR/Omarchy or Flatpak publication.

### 9: Linux-only repository extraction

- [x] Remove retired non-Linux source, tests, build projects, runtime driver bundles, documentation, and release automation
- [x] Keep the Cargo workspace under `linux/` to preserve stable build and packaging paths
- [x] Rewrite root and Linux documentation around the Rust and GTK architecture
- [x] Retain optional upstream behavior review without source-tree merging
- [x] Preserve the root license, Linux changelog, Linux workflows, and package scaffolding

The repository extraction completed on 2026-08-17. Product planning now follows Linux user needs and release evidence.

### 10: DBA operations at scale

- [x] Several connections open at once, with fail-closed per-tab ownership across all of them
- [x] Connection groups, tags, favorites, search, URL import, and environment colour on each row
- [ ] A typed sessions and locks console with capability-declared driver support and governed session termination
- [ ] A PostgreSQL server health panel that degrades cleanly when a statistics extension is absent
- [ ] Configurable pool size and timeouts per saved connection, honoured by the driver
- [x] Read-only review of views, materialized views, routines, triggers, sequences, extensions, roles, types and grants on PostgreSQL, through the Catalog window and the `list_objects` MCP tool (implemented; GTK unverified)
- [ ] A decision record, design, and measured prototype for an out-of-process Python runner

Phase 10 is in progress. Slice 10.2 added connection organisation: groups, tags, favourites, search across name/group/tag/driver, and URL import whose password reaches the keyring and never the saved file. Its first slice retired the one-active-connection limit: activation is additive and every window owns and releases its own connection, proven by two windows writing to two databases in the installed suite. Every connection it exposes stays policy-guarded, and no slice ships DDL or server configuration writes.

## Next implementation target

The 0.2 feature work in the [approved sprint](docs/bookie-0.2-sprint.md) is
implemented: editor file workflows, read-only PostgreSQL catalog and types, dedicated
editor sessions, the system OpenSSH transport, GSettings and history migrations, and
the narrowed column-collation contract. What remains is qualification: render every
new surface in [manual-verification-0.2-features.md](docs/manual-verification-0.2-features.md)
in light and dark, a real Flatpak build, and the release-candidate gates below.
Existing formatting, run-at-cursor, connection organization and DuckDB flat-file
opening must not be recreated from historical backlog entries.

RC release remains separate: freeze a candidate, collect 30 consecutive retry-free GTK attempts at that commit, and verify Arch install/upgrade/rollback under Wayland. The passing base-commit smoke job does not supply that ledger.
