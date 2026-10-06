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
| UI-8b | Second launch logs a GLib "did not unregister" warning | OPEN | Cosmetic | manual |
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
| UI-13 | Export covers loaded rows or the current page only | OPEN | Full-table streaming export, progress, snapshot semantics | driver-docker |
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
| UI-25 | CSV create-table uses PostgreSQL-leaning type names on ClickHouse, MongoDB, Redis, DuckDB | OPEN | Per-engine type map | driver-docker |
| UI-26 | Reusable SSH profiles are not editable; client certificates are partial | OPEN | With U5 below | gtk-widget |

## Transport, sessions, daemon (B4)

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| B4-1 | C6 MySQL: TLS through real SSH forwarding, wrong CA or host fails with no plaintext fallback | OPEN | `driver-tls` fixture | driver-docker |
| B4-2 | C6 SQL Server: same, needs a usable control connection | OPEN | | driver-docker |
| B4-3 | G5: daemon provider through system OpenSSH; unknown host key declines without learning | OPEN | | driver-docker |
| B4-4 | F8 headless: agentd shares one `AuditState`; needs per-connection generations | OPEN | `agentd/src/lib.rs` | sandbox |
| B4-5 | Retire the daemon handle after panic or disconnect even when ping succeeds | OPEN | Small patch before G5 | sandbox |
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
| B4-20 | Credential rotation during connect: fingerprint and assembly load material separately | OPEN | Race test | sandbox |
| B4-21 | SQL Server Kerberos and TLS need a deterministic KDC and certificate fixture | OPEN | | driver-docker |
| B4-22 | Bundle export and import write no audit entries | OPEN | Needs an admin-event class ADR | sandbox |

## Security

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| SEC-1 | Panic payload is still logged as `detail` | OPEN | Sentinel tests over tracing, stderr, errors, audit; ADR 0006 | sandbox |
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
| PKG-2 | Debian/GNOME phase UI-D1 to D4 | OPEN | After PKG-1 | gtk-installed |
| PKG-3 | B7 soak: frozen SHA, 30 consecutive retry-free GTK attempts over six runs | OPEN | | gtk-installed |
| PKG-4 | Real Flatpak build and install; Flathub submission; screenshots | OPEN | | manual |
| PKG-5 | `dpkg-deb` contract skipped in harness; validators tested, installation not | OPEN | Debian runner | sandbox |
| PKG-6 | `bookie` replaces `tablepro`: install, upgrade, removal must keep XDG data | OPEN | | manual |
| PKG-7 | Oracle ODPI build is unsupported and not shippable | ACCEPTED | Out of scope | n/a |

## Tests and evidence

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| TEST-1 | ~~File-size guard failing on the PostgreSQL scalar consumers file~~ | DONE | Guard passes on `linux`; baselines still need lowering | sandbox |
| TEST-1b | ~~Lower the file-size baselines the guard notes as shrunk~~ | DONE | `file-size-baselines.txt` ratcheted; `result_consumers_temporal.rs` (1,249 lines) now listed, still to be split by B3 | sandbox |
| TEST-2 | Mutation: core and package survivors untriaged; portable evidence | OPEN | B3-P6 | sandbox |
| TEST-3 | No line-coverage number | OPEN | `cargo-llvm-cov` on the runner | manual |
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
| TEST-24 | Forgejo also evaluates `.github/workflows` and warns that every job's `permissions` field is unsupported and ignored (preflight, fast, gtk-safety, current-stable-clippy, integration, driver-tls, postgres-release, duckdb, regression-gate); those jobs wait on an `ubuntu-24.04` runner that does not exist | OPEN | Scope Forgejo to `.forgejo/workflows`, or give the runner a Docker label; then use Authorized Integrations for any token capability the lab jobs need | manual |
| TEST-17 | ~~No check that documentation links resolve~~ | DONE | `scripts/check-doc-links.py` in the harness | sandbox |

## Documentation and other

| ID | Issue | Status | Next | Layer |
| --- | --- | --- | --- | --- |
| DOC-1 | `po/tablepro.pot` is stale | OPEN | Run `update-translations.py` in its own commit | manual |
| DOC-2 | ~~Adoption matrix is stale~~ | DONE | Archived; this ledger owns open items | manual |
| DOC-3 | Accessibility: Orca pass, keyboard-only order, high contrast, accessible names | OPEN | [accessibility](accessibility.md) | gtk-installed |
| DOC-4 | ~114 inline references to absent cache reports | ACCEPTED | Marked unavailable in the archive | n/a |
| DOC-5 | Cross-client value comparison (DBeaver, dbx) on lab VMs | OPEN | Seeded multi-engine VMs, ADR 0007 as oracle | manual |
| DOC-6 | ~~Too many top-level documents~~ | DONE | 30 dated documents moved to [archive](archive/) | n/a |
| PERF-1 | PostgreSQL capped result about 12% slower, 58% lower RSS | OPEN | Profile release binaries | manual |
| PERF-2 | Arbitrary results are materialized to caps, not streamed | OPEN | | driver-docker |
| PERF-3 | MongoDB census cost per browse not measured | OPEN | | driver-docker |
| PERF-4 | Timing shows elapsed only, never server time | OPEN | With UI-20 | driver-docker |

## Test environment

The Arch runner `bookie-ci` on the office testlab builds and tests this tree:
GTK 4.22, libadwaita 1.9, Rust 1.98, Docker, Xvfb, D-Bus, AT-SPI. Run any
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
