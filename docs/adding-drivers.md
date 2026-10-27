# Adding a database driver

Drivers are static Rust crates under `crates/drivers/`; see [ADR 0001](decisions/0001-no-plugin-system.md). The [0.2.0 completion page](bookie-0.2-sprint.md) records delivered engine scope; the [backlog](backlog.md) and owning boards set post-release sequencing and acceptance. Use [ADR 0007](decisions/0007-type-and-value-preservation.md) as the common type/value and native proof standard. The [documentation entry point](README.md) identifies the owning boards and selective evidence lookup. Recheck the current traits and implementations before using this checklist; the workspace manifests are authoritative for dependency versions.

## Contracts and crate boundaries

Read the actual traits before implementing a driver:

- [DatabaseDriver](../crates/core/src/driver.rs): identity, defaults and static capabilities. The typed profile and the rules for connection-scoped or metadata-derived behavior are in [static driver capabilities](#static-driver-capabilities).
- [Connection](../crates/core/src/connection.rs): schema-aware reads, bound parameters, transactions, controlled calls, session creation and optional query cursors.
- [Session](../crates/core/src/session.rs): dedicated connection lifetime, transactions, usability and awaited close.
- [Values](../crates/core/src/query.rs), [errors](../crates/core/src/error.rs) and [operation control](../crates/core/src/operation.rs).

Use an existing driver's implementation and manifest as the starting point. The actual trait definitions own the contract; an incomplete example can omit safety hooks. Unsupported defaults do not establish capability support.

Production drivers depend on `tablepro-core`, not GTK, Relm4, policy, MCP, storage, transport or sibling drivers. Shared saved-credential assembly and tunnel ownership belong in `tablepro-transport`; SSH backends belong in `tablepro-ssh`. Policy wraps the raw handle in the composition root.

## Existing libraries

These are the dependencies used by the current drivers:

| Engines | Library |
| --- | --- |
| PostgreSQL, MySQL/MariaDB, SQLite | `sqlx` with engine-specific features |
| SQL Server | `tiberius` (workspace crates.io `0.13`) |
| ClickHouse | `clickhouse` plus the current dynamic-result implementation |
| Redis | `redis`, including the workspace's vendored patch |
| MongoDB | `mongodb`, including the workspace's vendored patch |
| DuckDB | `duckdb`, behind the optional `duckdb` feature |

Review engine/library proposals against [driver maturity](adding-drivers.md#order-the-engines-were-added), dependency policy and license requirements. Keep driver registration static unless a new architecture decision explicitly changes that contract.

## Implementation checklist

1. Create `crates/drivers/<engine>/`. Package name is `tablepro-driver-<engine>`; existing library names use `drivers_<engine>`. Reuse workspace versions/lints and review dependencies with `deny.toml`.
2. Implement the current traits. Preserve SQL NULL versus unsupported/non-NULL values; unsupported values must remain `Value::Undecodable` or be refused, rather than becoming NULL. Map errors to typed `DriverError` variants.
3. Bind values and quote identifiers for the actual dialect. Carry schema and table identity separately. Declare generated/identity metadata accurately; include composite keys and real affected-row behavior.
4. Declare maturity and static capabilities honestly, including `enum_input_style` when grid and paste input need dialect-specific enum handling. Controlled calls need a bounded operation, confirmed server cancellation or an honest unknown outcome with retirement. Do not claim Stop support from a dropped client future.
5. If supporting sessions, prove settings/temp tables/transactions stay on one physical connection, lost sessions never fall back to a pool, and close is awaited. New methods must be forwarded through policy and owning wrappers.
6. Keep dial endpoints separate from TLS/Kerberos service identity. Test wrong CA/hostname, no plaintext fallback and tunneled verification per engine. Initialize the shared crypto provider in test composition roots that link multiple TLS drivers. Do not load saved secrets inside a driver.
7. Add the workspace member and dependencies in both composition roots. Register in [app::build_registry](../apps/linux/app/src/lib.rs) and [agentd::build_registry](../crates/agentd/src/main.rs). Apply matching feature gates where native build cost requires them.
8. Update maturity/connection documentation and the user-facing changelog. Register tests and runner ownership before claiming integration.
9. Test each server flavour on a real server. MariaDB 10.2.7 and later reports a column default as the SQL that defines it and a missing default as the text `NULL`, so re-quoting it would have created a default of `'NULL'`; only a real MariaDB container caught it ([sprint history](archive/bookie-0.2-history.md)). Detect flavour from the server's reported version, not from the driver name.
10. Do not let an engine-specific input parser bypass the shared edit rules. `parse_input_for_column` decides empty input (nullable becomes `Null`, required refuses) before any driver parser runs; a driver parser that runs first can turn an empty cell into a zero write (a MongoDB decimal once did, [B3 review](archive/b3-review-2026-10-01.md)). Test empty input for nullable, defaulted and required columns of every type the driver parses.

## Tests and evidence

Use current testcontainers `AsyncRunner` examples in existing drivers. Integration crates need the test-only Clippy allowances specified in `AGENTS.md`; fixture tests are ignored in the fast tier and executed by their dedicated layer.

From the repository root, a focused engine fixture command is:

```bash
cargo test -p tablepro-driver-<engine> --test integration -- --include-ignored --test-threads=1
```

Cover native stored type/value, invalid neighbors, bound parameters, consumer round trips, late stream failures, server loss and subsequent handle usability. Mocks cover dispatch/denial; real engine assertions establish server behavior. Record initial failing behavior before applying a bug fix.

Review `scripts/ci-local.sh`, `scripts/test-layers.json`, the TLS runner, change-test mapping, ignored-test inventory and hosted workflow package lists for a new crate. Register isolated GTK/keyring tests where needed. Run affected layers selected through [the validation playbook](validation-playbook.md); regenerate the ignored-test inventory. Optional-feature build evidence is separate from default-workspace success.

Return an exact SHA, commands, report paths, retained sanitized evidence and unrun gates. Driver maturity, a fixture pass, package qualification and release approval are separate claims.

## Static driver capabilities

`DatabaseDriver::capabilities()` provides a typed, compile-time profile for engine behavior that can shape shared UI, driver documentation and conformance tests. Drivers remain statically registered. The profile is an aggregate view of existing declarations; its default implementation reads the current trait methods so the same value is not maintained twice. Migrate consumers to the profile in small slices before considering removal of the individual methods.

### Static profile

| Profile field | Current declaration | Category and use |
| --- | --- | --- |
| `maturity` | `maturity()` | Driver completeness shown in the connection picker and documented in the driver table. |
| `enum_input_style` | `enum_input_style()` | Engine input semantics used by GTK-free enum parsing and browse-tab initialization. |
| `is_file_based` | `is_file_based()` | Engine connection shape used by the connection dialog and saved-file flow. |
| `ddl_is_transactional` | `ddl_is_transactional()` | DDL rollback guarantee used by structure-save orchestration. |
| `reports_rows_affected` | `reports_rows_affected()` | Driver guarantee used by optimistic grid writes. |
| `supports_database_listing` | `supports_database_listing()` | Whether the database switcher can enumerate databases. |
| `supports_index_metadata` | `supports_index_metadata()` | Whether index metadata is implemented rather than an empty-list default. |
| `supports_foreign_key_metadata` | `supports_foreign_key_metadata()` | Whether foreign-key metadata is implemented rather than an empty-list default. |
| `column_comment` | `column_comment_capability()` | Typed lifecycle and SQL placement for unsupported comments, new-column-only comments, or comments editable on both new and existing columns. |
| `supports_view_metadata` | `supports_view_metadata()` | Whether the driver implements view metadata. |
| `catalog_object_kinds` | `catalog_object_kinds()` | Static catalog kinds exposed by the driver. |
| `supports_integrated_auth` | `supports_integrated_auth()` | Authentication form and transport validation. |
| `supports_local_socket` | `supports_local_socket()` | Whether the connection form offers local-socket mode. |
| `supports_local_socket_file` | `supports_local_socket_file()` | Whether transport accepts a local socket file. |
| `supports_local_socket_directory` | `supports_local_socket_directory()` | Whether transport accepts a local socket directory. |
| `supports_client_tls_auth` | `supports_client_tls_auth()` | Whether transport accepts client certificate authentication. |
| `supports_explicit_auto_increment_values` | `supports_explicit_auto_increment_values()` | Whether CSV import may include explicit identity values in the insert plan and saved mapping. |

Current defaults are `Stable`, `RawValue`, non-file-based, non-transactional DDL, real affected-row counts, and false/empty for the remaining support flags. Column-comment capability defaults to `Unsupported`; its typed mapping also supplies the DDL placement. Current per-driver overrides are:

| Driver | Overrides from the defaults |
| --- | --- |
| ClickHouse | `enum_input_style = SqlQuotedLabel`, database listing, no affected-row count, index metadata, column comments on new columns only |
| DuckDB | `enum_input_style = SqlQuotedLabel`, `Experimental`, file-based, transactional DDL |
| MongoDB | `Experimental`, database listing |
| MySQL | `enum_input_style = DeclaredMemberList`, database listing, index and foreign-key metadata, client TLS auth, local socket and socket-file support, explicit auto-increment values, inline comments on new and existing columns |
| PostgreSQL | transactional DDL, database listing, index and foreign-key metadata, view metadata and catalog kinds, client TLS auth, local socket and socket-directory support, explicit auto-increment values, separate comments on new and existing columns |
| Redis | `Experimental` |
| SQL Server | database listing, transactional DDL, index and foreign-key metadata, integrated authentication, separate comments on new and existing columns |
| SQLite | file-based, transactional DDL, index and foreign-key metadata, explicit auto-increment values |

### Current consumers

| Declaration | Current code or documentation consumer |
| --- | --- |
| `maturity` | Connect dialog reads the typed profile in `apps/linux/app/src/ui/connect_dialog/mod.rs`; the driver maturity table lives in `adding-drivers.md`. |
| `enum_input_style` | Browse-tab initialization and GTK-free grid/paste parser. This is the first consumer migrated to `capabilities()`. |
| `is_file_based` | Connect dialog and duplicate-connection flow read the typed profile. |
| `ddl_is_transactional` | Structure-save orchestration reads the typed profile. |
| `reports_rows_affected` | Inline grid write handling reads the typed profile. |
| `supports_database_listing` | Database switcher reads the typed profile. |
| Integrated auth, local sockets and client TLS auth | The connect form reads the typed profile. GTK-free auth and endpoint visibility, plus file-versus-directory socket selection, live in `crates/app-services/src/connection_form.rs`; the form does not infer socket shape from an engine ID. Transport validation still reads the declarations directly and remains a backend consumer migration under [#1688](https://github.com/cozyGarage/BookiE/issues/1688). |
| `supports_explicit_auto_increment_values` | CSV import planning, preview mapping and saved mapping validation. The profile is passed at runtime; it is not stored in the saved mapping file. |
| `supports_index_metadata`, `supports_foreign_key_metadata` | `crates/app-services/src/structure_metadata.rs` skips unsupported metadata reads, preserves supported empty results, and applies cancellation/deadline checks between reads. The Structure editor labels unsupported metadata as unavailable instead of showing the same blank list as a supported empty result. These flags describe metadata reads only; DDL mutation availability remains governed by the DDL builder and driver behavior. |
| View metadata and catalog kinds | Deferred: profile construction and conformance tests read these fields, but no direct production consumer exists in the app or app-services. Keep the existing metadata methods and behavior; add a consumer only when a user-facing action has acceptance criteria that needs this distinction. Do not treat metadata support as proof the UI exposes it. |
| `column_comment` | The Structure editor reads the typed profile to decide whether comments are unsupported, editable only for new columns, or editable for existing columns. DDL placement comes from the same capability value. |

### Values outside the profile

Some engine behavior is not a static driver fact and stays at its current boundary:

- `Connection::supports_server_cancellation()` is connection-scoped. A live server or session can differ from the static driver's potential support.
- Per-object write permission comes from database metadata. Unknown permission remains unknown and is not inferred from the engine profile; see [#1065](https://github.com/cozyGarage/BookiE/issues/1065).
- `forwarded_socket_name(service_port)` is a parameterized transport resolver. Its output depends on the requested service port, so it remains a callable operation instead of a boolean profile field.
- Default port, database and username are connection-form defaults, not engine capabilities.

### Tests and migration

The registered-driver tests check the enum style consumed by the browse editor, compare every profile field against current declarations for every statically registered driver, and verify that the Structure editor's comment lifecycle matches the DDL capability mapping; each declared local-socket mode must have exactly one supported path shape. Structure tests distinguish unsupported index/foreign-key reads from supported empty results and verify the registered profiles reach the consumer. The app-services structure metadata tests cover unsupported-read skipping, supported empty results, error short-circuiting, cancellation between reads and expired deadlines. Installed PostgreSQL and ClickHouse scenarios in `apps/linux/app/tests/gtk_structure.py` verify supported empty/populated metadata and the unsupported-FK notice through the real driver/controller/view path. They run through `scripts/test-gtk-postgres.sh` and `scripts/test-gtk-clickhouse.sh`; the [October 26 local acceptance](evidence/structure-metadata-capabilities-2026-10-26/README.md) records the source, binary and results. The optional DuckDB assertion runs when that feature is enabled. GTK-free app-services tests cover auth and endpoint visibility, file and directory socket options, and refusal of ambiguous or inconsistent socket declarations. Core tests also cover a driver with no metadata declarations and a driver overriding every profile field.

When migrating another consumer, preserve the existing refusal and unsupported behavior. Shared tests must assert observable capability use and keep missing metadata distinct from supported-but-empty metadata. Remaining consumer migrations and their evidence are tracked by [#1688](https://github.com/cozyGarage/BookiE/issues/1688); the profile foundation and connection-form migration landed in [#1663](https://github.com/cozyGarage/BookiE/pull/1663) and [#1686](https://github.com/cozyGarage/BookiE/pull/1686).

## Driver maturity

Every registered driver reports a `DriverMaturity` (`Stable` or `Experimental`). The Connect dialog shows Experimental as a subtitle so users are not surprised by missing write or transaction paths. These are driver declarations, not release approval; see the [post-release backlog](backlog.md) and issue-owned acceptance criteria.

| Driver | Maturity | Connect | Browse | Query | Writes | Params | `begin` / MCP preview | Notes |
|---|---|---|---|---|---|---|---|---|
| PostgreSQL | Stable | yes | yes | SQL | yes | yes | yes | Five TLS modes; release fixture covers certificate verification and SSH routes |
| MySQL | Stable | yes | yes | SQL | yes | yes | yes | DDL not transactional; driver TLS fixture |
| SQLite | Stable | yes | yes | SQL | yes | yes | yes | File-based |
| SQL Server | Stable | yes | yes | SQL | yes | yes | no interactive begin | Tiberius; custom CA implemented; driver TLS fixture; Verify CA and Verify Full are identical; server cancellation unsupported |
| ClickHouse | Stable | yes | yes | SQL | yes | yes | no | Waits for asynchronous mutations to finish; no row counts; driver TLS fixture |
| Redis | Experimental | yes | DBs / SCAN | Redis CLI | via query | one bound key for the Value read | no | See [known limitations](known-limitations.md#editing-and-connections) for topology support; driver TLS fixture; Verify CA behaves as Verify Full |
| MongoDB | Experimental | yes | collections | find / aggregate | insertOne / deleteMany / drop; keyed grid edits use bound values | no | no | TLS fixture coverage; Verify CA behaves as Verify Full; integration coverage includes browse, find, aggregate, stale-edit and stale-delete checks |
| DuckDB | Experimental | yes | yes | SQL | yes | yes | no | Cargo feature `duckdb`; CSV/TSV/JSON/Parquet files; bundled JSON/Parquet; explicit optional CI |

### Maturity rules

1. Default maturity is `Stable`. Override only when a path users expect is missing or dialect-limited.
2. Do not register a driver that always fails at connect.
3. DuckDB stays behind `--features duckdb` for compile size, not maturity.
4. Raising a driver to Stable means browse, the engine's query dialect, common writes, and CI integration coverage are in place.

Transport and TLS behaviour per driver is tracked in [connections.md](connections.md).

### Order the engines were added

ClickHouse (Stable), Redis (Experimental), DuckDB (Experimental, `--features duckdb`), MongoDB (Experimental). SSH jump-host chains belong in `crates/ssh` as sequential tunnels, not in drivers.
