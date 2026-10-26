# Static driver capabilities

`DatabaseDriver::capabilities()` provides a typed, compile-time profile for engine behavior that can shape shared UI, driver documentation and conformance tests. Drivers remain statically registered. The profile is an aggregate view of existing declarations; its default implementation reads the current trait methods so the same value is not maintained twice. Migrate consumers to the profile in small slices before considering removal of the individual methods.

## Static profile

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

## Current consumers

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

## Values outside the profile

Some engine behavior is not a static driver fact and stays at its current boundary:

- `Connection::supports_server_cancellation()` is connection-scoped. A live server or session can differ from the static driver's potential support.
- Per-object write permission comes from database metadata. Unknown permission remains unknown and is not inferred from the engine profile; see [#1065](https://github.com/cozyGarage/BookiE/issues/1065).
- `forwarded_socket_name(service_port)` is a parameterized transport resolver. Its output depends on the requested service port, so it remains a callable operation instead of a boolean profile field.
- Default port, database and username are connection-form defaults, not engine capabilities.

## Tests and migration

The registered-driver tests check the enum style consumed by the browse editor, compare every profile field against current declarations for every statically registered driver, and verify that the Structure editor's comment lifecycle matches the DDL capability mapping; each declared local-socket mode must have exactly one supported path shape. Structure tests distinguish unsupported index/foreign-key reads from supported empty results and verify the registered profiles reach the consumer. The app-services structure metadata tests cover unsupported-read skipping, supported empty results, error short-circuiting, cancellation between reads and expired deadlines. The optional DuckDB assertion runs when that feature is enabled. GTK-free app-services tests cover auth and endpoint visibility, file and directory socket options, and refusal of ambiguous or inconsistent socket declarations. Core tests also cover a driver with no metadata declarations and a driver overriding every profile field.

When migrating another consumer, preserve the existing refusal and unsupported behavior. Shared tests must assert observable capability use and keep missing metadata distinct from supported-but-empty metadata. Remaining consumer migrations and their evidence are tracked by [#1688](https://github.com/cozyGarage/BookiE/issues/1688); the profile foundation and connection-form migration landed in [#1663](https://github.com/cozyGarage/BookiE/pull/1663) and [#1686](https://github.com/cozyGarage/BookiE/pull/1686).
