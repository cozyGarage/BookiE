# Known issues ledger

The single live list of open work outside the B3 type/value matrix. It replaces
the per-review "remaining" sections in [docs/archive](archive/); those files stay
as dated evidence and are not updated.

Rules:

- A closed issue is crossed out (`~~text~~`) and carries the commit, PR or test
  that closed it. A fix lands with its regression test in the same change.
- Status is `OPEN`, `DONE`, `UNVERIFIED` (claimed but not proven here), or
  `ACCEPTED` (a deliberate limit, with the reason).
- Layers are the tiers in [testing](testing.md) and the
  [validation playbook](validation-playbook.md). Nothing is `DONE` on a source
  read alone.
- B3 stays on its own board: [type/consumer board](type-contract-strategy.md)
  and [value evidence index](value-contracts.md). Remaining: broader
  engine/type/consumer/configuration matrix, mutation triage, installed grid
  acceptance.

Source snapshot: `origin/linux` after PR #91, 2026-10-06. Items come from the
archived audits of 2026-09-17 to 2026-10-06 and the
[sprint](bookie-0.2-sprint.md); the first column says which area owns them.

## UI (app layer)

| ID | Issue | Status | Evidence / next | Layer |
| --- | --- | --- | --- | --- |
| UI-1 | ~~Saved connections cannot be edited~~ | DONE | PR #87, `prefill.rs` tests | unit |
| UI-1b | SSH jump chains cannot be edited in the form; such connections are refused for editing | OPEN | Needs a chain editor in the form | gtk-widget |
| UI-2 | ~~Failed connect has no Retry or Edit~~ | DONE | PR #87, `recovery_tests` | unit |
| UI-3 | ~~Saved connection cannot be cancelled while connecting~~ | DONE | PR #87, `cancellation_tests` | unit |
| UI-3b | ~~The connect dialog's own Connect button has no cancel~~ | DONE | `open_candidate` cancelled before anything is saved; scenario `connect_dialog_cancel_stops_a_hanging_connection` | gtk-installed |
| UI-4 | ~~New connection form defaults to ClickHouse; no file pickers; bare TLS error~~ | DONE | PR #87 | unit |
| UI-5 | ~~Test Connection result is a transient raw toast~~ | DONE | `test_result_text` test; scenarios `test_connection_reports_success_in_the_dialog` and `..._failure_...` | gtk-widget |
| UI-6 | ~~`Ctrl+/` opened the shortcuts window instead of toggling an editor comment~~ | DONE | `window_shortcut_tests`; scenario `ctrl_slash_toggles_a_comment_in_the_editor` | gtk-installed |
| UI-7 | ~~Editor has no find or replace~~ | DONE | PR #87, `find_bar` widget test | gtk-widget |
| UI-7b | ~~Find has no case or regex toggle~~ | DONE | `find_bar` widget test covers match case and regex | gtk-widget |
| UI-8 | ~~Second launch exits silently~~ | DONE | PR #87, verified with D-Bus on the runner | gtk-installed |
| UI-8b | ~~Second launch logged a GLib "did not unregister" warning~~ | DONE | Second launch runs through `Application::run`; scenario `a_second_launch_raises_the_window_and_exits_cleanly` | gtk-installed |
| UI-9 | ~~No read-only viewer for long, JSON or binary cell values~~ | DONE | PR #91, `value_viewer` tests | unit + gtk-widget |
| UI-9b | No row inspector side pane | OPEN | `AdwOverlaySplitView` trailing pane | gtk-widget |
| UI-27 | ~~Editable result cells opened two menus on right-click~~ | DONE | Capture-phase gesture; end-to-end scenarios `view_value_*` and `columns_dialog_*` | gtk-installed |
| UI-28 | ~~The grid cell menu had no standard keyboard shortcut~~ | DONE | Shift+F10 added beside the Menu key | gtk-installed |
| UI-29 | ~~Editing a connection failed outright when the keyring was unavailable or its unlock was cancelled~~ | DONE | `prefill.rs` `readable` test; scenario `editing_a_saved_connection_*` | unit + gtk-installed |
| UI-30 | ~~Find and replace fields had no accessible name~~ | DONE | Labels added; scenario `find_bar_replaces_every_match_in_the_editor` | gtk-installed |
| UI-31 | ~~Every cell edit on a SQLite or DuckDB table asked for a manual write approval~~ | DONE | `policy/tests/generated_updates_classify.rs` across five engines; scenario `browse_edit_cell_and_save_persists_to_the_database` | unit + gtk-installed |
| UI-32 | ~~Every grid edit or delete on PostgreSQL, MySQL and SQL Server asked for approval (`blast_radius_unknown`) because the count query ran without its bound values~~ | DONE | `guard_tests_checked.rs` (driver-enforced one-row bound, fail-closed cases kept); `checked_write_tests`; scenario `postgres_grid_edit_and_delete_commit_to_the_server` | unit + gtk-installed |
| UI-10 | Multi-statement results use one switcher button per statement | OPEN | Named, pinnable result tabs in `outcomes.rs` | gtk-widget |
| UI-11 | Sidebar is a flat tables and views list; other objects only in the Catalog window | OPEN | `GtkTreeListModel` tree | gtk-widget |
| UI-12 | ~~No database or schema switcher~~ | DONE | `Connection::list_databases` plus guard audit test; five driver tests against real servers; scenario `postgres_database_switcher_reconnects_to_the_chosen_database` | unit + driver-docker + gtk-installed |
| UI-13 | ~~Export covers loaded rows or the current page only~~ | DONE | The browse export menu has Export all rows as CSV or JSON: pages of 5,000 rows through the same connection guard, a background job with progress and cancel that publishes the file only when complete, a CSV null-marker collision refused. Tests `export::paged` and `services::export_pages` (12,003 rows over SQLite, filter, error). Snapshot consistency is UI-13b | unit + sqlite |
| UI-13b | A full export is not a snapshot: rows changed during the export can be missed or repeated | OPEN | Read all pages inside one session transaction (ADR 0008); the dialog says so until then | driver-docker |
| UI-14 | ~~No way to hide grid columns~~ | DONE | `column_widths.rs` and `column_visibility.rs` tests | unit + gtk-widget |
| UI-14b | Hidden columns are still fetched; column order is not saved; no find in loaded rows | OPEN | Reorder, then grid search bar | gtk-widget |
| UI-15 | Preferences lack theme override, null style, editor font family, vim mode | OPEN | `preferences.rs`, `AdwStyleManager` | gtk-widget |
| UI-16 | No statement navigation or current-statement band in the editor | OPEN | `statement_cursor.rs` boundaries | gtk-widget |
| UI-17 | No preview tabs; `Ctrl+Tab` is not most-recent-first | OPEN | `workspace_tabs.rs` | gtk-widget |
| UI-18 | History is a dialog; Open Quickly has no scopes or commands; no single action table | OPEN | `shortcuts.rs` action table | gtk-widget |
| UI-19 | FK picker and navigation, enum and set pickers, paste TSV, page-size menu, estimated counts | OPEN | B3-hot files; schedule with B3 | gtk-widget |
| UI-20 | No server output (NOTICE, PRINT) and no timing breakdown | OPEN | Needs a core trait hook | driver-docker |
| UI-21 | No `CellView` display boundary and no fake connection for the GTK tier | OPEN | Foundation packets in the archived audit | unit |
| UI-22 | Code folding, vim, multi-cursor, split panes | OPEN | Vim is a preference; the rest need feasibility slices | manual |
| UI-23 | PostgreSQL catalog: materialized views, routines, triggers, sequences, extensions, roles; typed activity console | OPEN | B6 | driver-docker |
| UI-24 | Workspace restore proven only partially | UNVERIFIED | Restart with every referenced connection | gtk-installed |
| UI-25 | ~~CSV create-table uses PostgreSQL-leaning type names on ClickHouse, MongoDB, Redis, DuckDB~~ | DONE | Creating a table from a file is offered only on PostgreSQL, MySQL, SQLite, SQL Server and DuckDB (ClickHouse needs an ENGINE clause, MongoDB and Redis have no tables); DuckDB spells JSON as `JSON`. Tests `import::infer` | unit |
| UI-26 | Reusable SSH profiles are not editable; client certificates are partial | OPEN | With U5 below | gtk-widget |

## Transport, sessions, daemon (B4)

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| B4-1 | C6 MySQL: TLS through real SSH forwarding, wrong CA or host fails with no plaintext fallback | OPEN | `driver-tls` fixture | driver-docker |
| B4-2 | C6 SQL Server: same, needs a usable control connection | OPEN | | driver-docker |
| B4-3 | G5: daemon provider through system OpenSSH; unknown host key declines without learning | OPEN | | driver-docker |
| B4-4 | ~~F8 headless: agentd shared one `AuditState` across every connection~~ | DONE | Each cached session owns an audit generation; `audit_isolation_tests` proves A is blocked after an interrupted write, B stays writable and a replacement recovers (fails with the old shared state) | sandbox |
| B4-5 | ~~Retire the daemon handle after a driver panic or disconnect even when ping succeeds~~ | DONE | `SessionFaultSink` marks the cached session retired through the guard; `a_session_whose_driver_panicked_is_not_reused_even_though_its_ping_is_healthy` (fails without the change) | sandbox |
| B4-6 | ~~I2: Flatpak plus system OpenSSH must refuse explicitly before dispatch~~ | DONE | `TransportError::SystemSshUnavailableInSandbox` raised in `build_openssh_config` before any process; `sandbox_tests` | sandbox |
| B4-7 | I5: tunnel setup and host-key refusal audited with one terminal outcome each | OPEN | | sandbox |
| B4-8 | F7: isolated GTK Session, BEGIN, label, toggle-off confirm | OPEN | Register in `isolated-tests.json` | gtk-widget |
| B4-9 | I3: reconcile route, auth and TLS evidence after C6, G5, I2 | OPEN | Docs | manual |
| B4-10 | ~~I1: `packaging/debian/rules` has no askpass build or install~~ | DONE | `packaging/debian/rules` builds and installs `tablepro-askpass`; the validator and `test_deb_package.py` reject a package without it | sandbox |
| B4-11 | MySQL batch: only InnoDB and the trigger boundary are proven | OPEN | Other engines and side effects | driver-docker |
| B4-12 | PostgreSQL rollback-failure acceptance | OPEN | Native fixture | driver-docker |
| B4-13 | ~~U4: reconnect retried every error forever~~ | DONE | `is_permanent_failure` tests; `a_credential_failure_ends_the_retry_loop_and_reports_the_reason`; `ConnectionHealth::Failed` shown in the banner. Raw error text still goes through `error_text` only | unit |
| B4-14 | U5: no client certificate or key in transport or storage | OPEN | Scope drivers and routes first | driver-docker |
| B4-15 | O1: connect A, cancel, switch to B, namespace ownership races | OPEN | | gtk-widget |
| B4-16 | F4/F9 stale-session invalidation merged but not accepted | UNVERIFIED | Installed acceptance | gtk-installed |
| B4-17 | F6: native multi-hop, cancellation, installed trust flow | OPEN | | driver-docker |
| B4-18 | Connect and Test Connection read tables on a raw connection before hand-out, with no audit record | OPEN | A decision for ADR 0008, not a bypass | sandbox |
| B4-19 | Hostile-server fixtures: real short or non-ASCII SCRAM nonces and excessive iterations | OPEN | Source-string tests only today | driver-docker |
| B4-20 | ~~Credential rotation during connect: fingerprint and assembly load material separately~~ | DONE | Daemon: `a_key_material_rotation_during_connect_is_not_cached_under_the_stale_digest` and `a_material_lookup_failure_*` tests; the GUI reconnect reuses the options captured at connect, so it has no second lookup | sandbox |
| B4-21 | SQL Server Kerberos and TLS need a deterministic KDC and certificate fixture | OPEN | | driver-docker |
| B4-22 | Bundle export and import write no audit entries | OPEN | Needs an admin-event class ADR | sandbox |

## Security

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| SEC-1 | ~~Panic payload was logged as `detail`~~ | DONE | `a_driver_panic_message_never_reaches_the_logs_or_the_error` (fails before); the guard logs only the message length; `withhold_panic_messages` replaces the default stderr hook in the app and agent (opt out with `TABLEPRO_DEBUG_PANICS=1`). Audit and MCP error paths already return the fixed text | sandbox |
| SEC-2 | U2: Copy as SQL keeps auto-increment values; IDENTITY and GENERATED ALWAYS need an engine-aware default | OPEN | | driver-docker |
| SEC-3 | U1: SQL Server server-owned columns proven for metadata and two INSERT paths only | OPEN | Grid, CSV import, Copy as SQL, SQL export | driver-docker |
| SEC-4 | T-SQL batches: `SELECT 1` then UPDATE or DROP; MERGE alone or final | UNVERIFIED | Native verification | driver-docker |
| SEC-5 | ~~`cargo-audit` and `cargo deny` not run locally; RUSTSEC-2023-0071 ignored~~ | DONE | `cargo deny check` on the Arch runner: advisories, bans, licenses and sources ok; the RUSTSEC-2023-0071 ignore stays documented in `deny.toml` | manual |
| SEC-6 | PostgreSQL result cap is client side only | OPEN | Never append LIMIT blindly | driver-docker |
| SEC-7 | Durable audit filesystem work can outlive the MCP deadline | ACCEPTED | Fails closed by design | n/a |
| SEC-8 | Trusted production mutations, unattended MCP writes and a public package are not approved | ACCEPTED | Decision, not work | n/a |

## Packaging and qualification

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| PKG-1 | Arch installed pass on native Wayland: install, upgrade, rollback, askpass, GSettings, profile isolation | OPEN | Xvfb does not qualify | gtk-installed |
| PKG-2 | Debian/GNOME phase UI-D1 to D4 | OPEN | Build, unit and widget tiers pass on Ubuntu 24.04 (GTK 4.14, libadwaita 1.5) and Debian 13 (GTK 4.18, libadwaita 1.7) in containers on the runner (`scripts/test-distro-floor.sh`, CI job `distro-floor`). The installed AT-SPI suite on Ubuntu 24.04 (`DISTRO_FLOOR_INSTALLED=1`) now passes its first 30-odd scenarios and renders correctly (screenshot checked). It found and fixed two older-stack gaps: a Status-role label that GTK 4.14 does not list, and a repeated toolbar content swap. It stops at `view_value_opens_the_whole_cell_with_pretty_json`: the grid cell context menu does not open there. Debian 13 installed run not done | gtk-installed |
| PKG-3 | B7 soak: frozen SHA, 30 consecutive retry-free GTK attempts over six runs | OPEN | | gtk-installed |
| PKG-4 | Real Flatpak build and install; Flathub submission; screenshots | OPEN | | manual |
| PKG-5 | `dpkg-deb` contract skipped in harness; validators tested, installation not | OPEN | Debian runner | sandbox |
| PKG-6 | `bookie` replaces `tablepro`: install, upgrade, removal must keep XDG data | OPEN | | manual |
| PKG-7 | Oracle ODPI build is unsupported and not shippable | ACCEPTED | Out of scope | n/a |

## Tests and evidence

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| TEST-1 | ~~File-size guard failing on the PostgreSQL scalar consumers file~~ | DONE | Guard passes on `linux`; baselines still need lowering | sandbox |
| TEST-1b | ~~Lower the file-size baselines the guard notes as shrunk~~ | DONE | `file-size-baselines.txt` ratcheted; B3 has since split `result_consumers_temporal.rs` under the limit, so its baseline entry is removed | sandbox |
| TEST-2 | Mutation: core and package survivors untriaged; portable evidence | OPEN | B3-P6 | sandbox |
| TEST-3 | ~~No line-coverage number~~ | DONE | 2026-10-07 on the runner: `cargo llvm-cov --workspace --exclude tablepro-driver-duckdb --lib --bins` reports 58.1% of lines (47,493 of 112,706 uncovered), 58.7% of functions. Unit tier only: driver integration, GTK and installed tiers are not counted. Examples: storage `connections.rs` 94%, `secrets.rs` 55%, ssh `supervisor.rs` 0% | manual |
| TEST-3b | Coverage is measured by hand, with no trend or floor | OPEN | The nightly Forgejo workflow runs `cargo llvm-cov` and fails below `coverage-floor.txt` (57). Not yet run on the schedule; trend storage still missing | sandbox |
| TEST-4 | Full driver, TLS and SSH matrix not rerun on one candidate tree | OPEN | Run on the runner; see below | driver-docker |
| TEST-5 | Hosted DuckDB container ownership fix not confirmed | UNVERIFIED | | manual |
| TEST-6 | 117 unchecked items in [manual verification](manual-verification-0.2-features.md) | OPEN | Automate what GTK automation can reach | gtk-installed |
| TEST-7 | No bundle export and import round trip between two real profiles | OPEN | | gtk-installed |
| TEST-8 | Process-restart and JSON-export GTK scenarios: installed proof pending | UNVERIFIED | | gtk-installed |
| TEST-9 | O2: SQL Server `max` text and binary at 64 KiB and MB sizes | OPEN | | driver-docker |
| TEST-10 | O3: multiple result sets, later sets are discarded | OPEN | Design across core, driver, guard, MCP, GUI | driver-docker |
| TEST-11 | ~~U3: completion conflated same-named tables in different schemas~~ | DONE | `table_key` keeps the schema; an ambiguous bare name offers no columns; three new `completion` tests replace the one that pinned the defect | unit |
| TEST-12 | U6: MongoDB metadata scan cost, writes to read documents, cancellation | OPEN | | driver-docker |
| TEST-13 | MongoDB nested filters, binary UUID, MQL; Redis Sentinel and Cluster | OPEN | | driver-docker |
| TEST-14 | Keep refusing PK-less delete; no all-column fallback | ACCEPTED | Guard to preserve | unit |
| TEST-15 | Lost-ack writes: reconnect success must not authorize replay | OPEN | Independent row and audit oracle | driver-docker |
| TEST-16 | Mongo hostile-server handshake not run end to end | OPEN | | driver-docker |
| TEST-18 | ~~The installed GTK suite had no scenarios for connection edit, find, value viewer or columns~~ | DONE | `gtk_ux.py`: four scenarios pass on the Arch runner | gtk-installed |
| TEST-19 | ~~The installed GTK suite assumed no keyring and the old default driver~~ | DONE | Fixed in this change: isolated unlocked keyring in `test-gtk-safety.sh`, PostgreSQL title; keep both | gtk-installed |
| TEST-20 | ~~No scenario proved the browse edit-and-save loop against a database with an independent oracle~~ | DONE | `browse_edit_cell_and_save_persists_to_the_database` (SQLite, real key presses, `sqlite3` read-back) | gtk-installed |
| TEST-21 | ~~No installed-GTK scenario against a real PostgreSQL server~~ | DONE | `scripts/test-gtk-postgres.sh`: saved connection, keyring password, live rows, value viewer | gtk-installed |
| TEST-22 | ~~The runner's default virtual CPU hid AVX, so MongoDB 7 containers exited and 29 driver tests failed~~ | DONE | `--cpu host`; MongoDB suite 31 of 31 on the runner | driver-docker |
| TEST-23 | ~~No installed-GTK scenario against a real MySQL server~~ | DONE | `scripts/test-gtk-mysql.sh`: edit and delete reach the server, no approval prompt | gtk-installed |
| TEST-24 | ~~Forgejo evaluated the GitHub workflows (unsupported `permissions` warnings, waiting on a missing `ubuntu-24.04` runner)~~ | DONE | Only the first push, before `.forgejo/` existed, did; later pushes run only `.forgejo/workflows/ci.yml`, whose quick layer ran on the Arch runner and caught a real function-size failure. Five stale waiting runs (1 to 5) can be cancelled in the Forgejo UI | manual |
| TEST-17 | ~~No check that documentation links resolve~~ | DONE | `scripts/check-doc-links.py` in the harness | sandbox |

## Documentation and other

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| DOC-1 | ~~`po/tablepro.pot` is stale~~ | DONE | Regenerated 2026-10-07 with `scripts/update-translations.py` in its own commit; 24 more source files are now listed in `POTFILES.in` | manual |
| DOC-2 | ~~Adoption matrix is stale~~ | DONE | Archived; this ledger owns open items | manual |
| DOC-3 | Accessibility: Orca pass, keyboard-only order, high contrast, accessible names | OPEN | [platforms](platforms.md#accessibility) | gtk-installed |
| DOC-4 | ~114 inline references to absent cache reports | ACCEPTED | Marked unavailable in the archive | n/a |
| DOC-5 | Cross-client value comparison (DBeaver, dbx) on lab VMs | OPEN | Seeded multi-engine VMs, ADR 0007 as oracle | manual |
| DOC-6 | ~~Too many top-level documents~~ | DONE | 30 dated documents moved to [archive](archive/) | n/a |
| PERF-1 | PostgreSQL capped result about 12% slower, 58% lower RSS | OPEN | Profile release binaries | manual |
| PERF-2 | Arbitrary results are materialized up to three caps (1M rows, 10M cells, 64 MiB estimated bytes), not streamed | OPEN | Baseline 2026-10-07 below: the byte cap binds first; GUI memory is about 2.5x the driver result; no streaming yet | driver-docker |
| PERF-7 | The grid deep-cloned every result row into its own GObject up front, so a 100k-row result held the rows twice | OPEN | Rows are now shared with the result and wrapped only when shown: 100k rows went from 308 MB to 240 MB resident (+150 to +82 MB over idle), 300k capped from 339 to 254 MB, measured on the runner. Remaining: the 61 MB driver result and the browse tab export copy. Target of 50% less not yet met | gtk-widget + profile |
| PERF-8 | Editor results are materialized in one go up to the caps; dbx pages 100 rows by default and appends more through a server-side cursor session on scroll | OPEN | Cursor-style progressive loading for editor queries: classify and audit once at open, bounded pages, cancel reaches the server. Needs a core design and an ADR note | driver-docker |
| PERF-9 | Hidden columns are still selected and transferred (dbx drops them from the query) | OPEN | Part of UI-14b: remove hidden non-key columns from the browse SELECT | unit + driver-docker |
| PERF-10 | Long cell values are shipped whole to the grid (dbx sends a preview with the byte count and fetches the full value on demand) | OPEN | Preview plus on-demand full value through View Value, behind the guard | driver-docker |
| PERF-5 | The idle app uses about 158 MB resident (Xvfb software rendering, eight drivers linked) | OPEN | Measure on real GPU rendering and with fewer drivers; see baseline below | manual |
| PERF-6 | Release binary is 68 MB on disk (34.7 MiB of code; the rest is symbols); largest crates are `tablepro_app`, `std`, `mongodb`, `sqlparser`, `zbus` | OPEN | Decide on stripping and the MongoDB cost | manual |
| PERF-3 | MongoDB census cost per browse not measured | OPEN | | driver-docker |
| PERF-4 | Timing shows elapsed only, never server time | OPEN | With UI-20 | driver-docker |

### What the reference client does differently

Read from dbx at `d9338d1` (a Tauri and Vue app over a Rust core), source only;
none of it was run. Its speed comes from holding and drawing less:

- pages of 100 rows by default, a 10,000-row fetch ceiling, and more rows
  appended through a server-side cursor session as the user scrolls;
- result rows marked raw (`markRaw`), so no per-row reactive objects exist;
- truncated cell previews with the full value fetched on demand, and hidden
  columns left out of the query;
- a canvas renderer with a fixed row height that paints only visible cells.

Our grid has the same virtualization (`GtkColumnView`), but wraps each row in a
GObject cloned from the result. PERF-7 to PERF-10 apply the same restraint.

### Performance baseline, 2026-10-07

Measured on the Arch runner (10 vCPU, host CPU type) with release builds, from
`scripts/profile-baseline.sh`. Rows have six columns of mixed integer, real and
text values (about 0.6 KB each).

| Level | Rows requested | Rows shown | Time | Memory |
| --- | ---: | ---: | ---: | ---: |
| Driver `query` (SQLite) | 10,000 | 10,000 | 0.12 s | +7 MB |
| | 100,000 | 100,000 | 0.8 s | +61 MB |
| | 500,000 | 120,019 (capped) | 0.95 s | +73 MB |
| | 1,000,000 | 120,019 (capped) | 1.5 s | +73 MB |
| App, result in the grid | 10,000 | 10,000 | 1.1 s to show | 194 MB resident (158 idle) |
| | 100,000 | 100,000 | 2.5 s | 308 MB |
| | 300,000 | 120,019 (capped) | 3.2 s | 339 MB |
| | 1,000,000 | 120,019 (capped) | 3.3 s | 334 MB |

- Allocation profile at 100,000 rows: 4.44 million allocations (about 44 per
  row), 59 MB peak heap.
- The 64 MiB byte budget binds long before 1,000,000 rows for text-heavy rows,
  so memory is bounded, but each shown row costs about 1.5 KB in the app against
  0.6 KB in the driver result: the grid holds a second copy.
- The Xvfb run uses software rendering; resident size on a real GPU session
  will differ and still needs a measurement.

## Test environment

The Arch runner `bookie-ci` on the office testlab builds and tests this tree:
GTK 4.22, libadwaita 1.9 (the code builds against the Ubuntu 24.04 floor), Rust 1.98, Docker, Xvfb, D-Bus, AT-SPI. Run any
command at a checkout's HEAD with the helper in the lab repository,
`scripts/bookie-ci-run.sh <checkout> '<command>'`. Tiers that pass there are
recorded per PR; none of them replaces the installed Wayland pass (PKG-1).

A lab Forgejo (VM 250 on pmox-lab03, `http://192.168.1.246:3000`, repository
`trung/bookie`, private) mirrors `linux` and runs `.forgejo/workflows/ci.yml`
on the `arch` runner (`forgejo-runner` as a systemd service on `bookie-ci`, host
mode, capacity 1, shared cargo cache). It is a visual lab copy of the checks
above, not a replacement for the GitHub gates.

The VM must use the `host` CPU type: the default virtual CPU hides AVX and
MongoDB 5 and later will not start. `scripts/test-gtk-postgres.sh` and the
Docker driver suites (`scripts/ci-local.sh integration`) run there.
