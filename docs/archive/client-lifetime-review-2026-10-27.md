# Code audit and client lifetime review, October 27, 2026

## Scope and evidence

Two parts, both against BookiE `linux` at `367006fe626d6ccb069508f032a754219d15482d`:

1. A read-only code audit of the B3 (core, drivers), B4 (policy, storage, transport, SSH, MCP, agentd) and UX (app, app-services) areas, plus documentation drift. Findings were spot-checked against the code before filing; none was reproduced at runtime.
2. A review of what peer clients fixed over their whole history, how they are built and how they test. Earlier reviews ([October 26](database-client-review-2026-10-26.md), [October 15 dbx](2026-10-15-dbx-review.md), [October 20](tablepro-dbx-review-2026-10-20/README.md)) compared features; this one looks at fix classes, practices and tests. New peers: DbGate, Antares SQL, HeidiSQL, Sequel Ace, Azure Data Studio, pgcli, mycli, litecli, usql, harlequin, rainfrog and gobang. Driver libraries were checked against crates.io, upstream changelogs and RustSec.

The [source manifest](../evidence/client-lifetime-review-2026-10-27/manifest.json) pins every repository. No third-party code was built or run. Fix-class shares are approximate keyword counts over `git log` or issue search, not a classification of every commit. DBeaver history came from issue search because a full clone did not fit the workspace.

## Audit results

| Area | Issues | Most significant |
| --- | --- | --- |
| B3 core and drivers | [#1707](https://github.com/cozyGarage/BookiE/issues/1707) to [#1717](https://github.com/cozyGarage/BookiE/issues/1717), [#1739](https://github.com/cozyGarage/BookiE/issues/1739), [#1743](https://github.com/cozyGarage/BookiE/issues/1743) | ClickHouse grid edits key on a non-unique sort key and are not row-count checked (#1739); SQL Server results can drop rows from the middle (#1711); MariaDB empty text written as NULL (#1707); FLOAT and `real` edits always conflict (#1708) |
| B4 policy, storage, MCP, SSH | [#1718](https://github.com/cozyGarage/BookiE/issues/1718) to [#1727](https://github.com/cozyGarage/BookiE/issues/1727), [#1740](https://github.com/cozyGarage/BookiE/issues/1740) to [#1742](https://github.com/cozyGarage/BookiE/issues/1742) | History export writes connection names into SQL comments unescaped (#1721); blast-radius count runs outside the session (#1726); JSON keys are not masked for agents (#1727); russh client security fixes (#1742) |
| UX app | [#1728](https://github.com/cozyGarage/BookiE/issues/1728) to [#1737](https://github.com/cozyGarage/BookiE/issues/1737), [#1744](https://github.com/cozyGarage/BookiE/issues/1744) | Health ping treats a busy pool as a lost connection (#1735); history rows and grid edit popovers leak (#1728, #1733); deleted connections' drafts come back (#1730) |
| Documentation | [#1738](https://github.com/cozyGarage/BookiE/pull/1738) | Cancellation per engine, the result-page spool, workspace locking, draft privacy, MCP scope names |

## Fix classes over the peers' lifetimes

Across DBeaver, TablePro, dbx and the GUI peers, the largest lifetime classes are export/import (about 7 to 8 percent of fix commits) and per-engine metadata (6 to 15 percent), followed by value and type round trips (3 to 6 percent). NULL versus empty text, edits hitting the wrong row and time zones are small by count but return with every new engine or code path. CLI clients add statement splitting, completion and Ctrl+C cancellation as recurring classes.

BookiE overlaps: #1707 (NULL versus empty), #1708 and #1716 (operators per type), #1710 (time zones), #1711, #1713 and #1722 (large results and memory), #1718 (timeouts), #1726 (transactions), #1739 (wrong-row edits) and #1740 (statement splitting) all belong to classes the peers fixed repeatedly.

Exposures that need qualification before they become issues:

- Direct database sockets have no TCP keepalive, `TCP_USER_TIMEOUT` or suspend and network-change handling; only SSH sets keepalive.
- The vendored sqlx sets the PostgreSQL session `TimeZone=UTC` (`vendor/sqlx-postgres/src/connection/establish.rs`); no test pins it and the UI does not show it.
- An editor session can hold an open transaction indefinitely; DBeaver's monitor warns and can end such transactions.
- Exports of keyless tables can page with `LIMIT`/`OFFSET` without `ORDER BY`.

## Practices

Adopt or adapt:

- A typed error classification (aborted transaction, lost connection) mapped from SQLSTATE in drivers and used for messages and session state, never for retry ([ADR 0008](../decisions/0008-connection-and-session-ownership.md)).
- A per-driver, per-value-kind table of exact operators as static data beside the driver capabilities, instead of point fixes per engine. It stays within [ADR 0001](../decisions/0001-no-plugin-system.md).
- One liveness policy with per-operation deadlines (TablePro), applied to direct sockets as well as SSH.
- A borrow-scoped rule for shared GTK-side state: borrow only within one method body, call no GTK or callbacks while borrowed, return owned changes (TablePro registries).
- A single statement splitter shared by the editor and the policy (pgcli and usql each keep one splitter).

Skip: DBeaver's automatic retry after recoverable errors (conflicts with ADR 0008), virtual keys for keyless tables, and dbx's move from sqlx to `tokio-postgres`.

## Testing and verification

- **Real engines over mocks.** DBeaver Community relies mostly on mocked unit tests and keeps fixing the same value, time zone and NULL classes. The peers with fewer such repeats test against real engines. BookiE's rule against mocking driver behaviour is the right one.
- **Version matrix.** TablePro runs each driver against the oldest and newest supported server. BookiE mostly pins one version per engine.
- **One contract suite per behaviour, instantiated per engine.** Beekeeper Studio (`tests/vitest/integration/lib/db/clients/all.js`) and DbGate (`integration-tests/engines.js`) run the same grid-write cases on every engine.
- **Regression test per fix, measured.** TablePro's share of fix commits that touch a test rose from 49 percent to 90 percent after it wrote the rule down; BookiE's measured share is about 60 percent since September (inline test modules are undercounted).
- **Bounded waits and snapshots for UI tests.** harlequin keeps one module of named waits with a single settle duration and compares screen snapshots.
- **Recorded transcripts.** usql compares CLI output against recorded files; the same style suits `bookie-agentd` MCP tool output.
- **Pooler fixture.** pgcli tests behind PgBouncer.

## Recommendations

| # | Recommendation | Owner | Tracking |
| --- | --- | --- | --- |
| 1 | Stop ClickHouse grid edits from using the MergeTree sort key as row identity, or check one affected row | B3, UX | [#1739](https://github.com/cozyGarage/BookiE/issues/1739) |
| 2 | Upgrade russh to 0.65.0 | B4 | [#1742](https://github.com/cozyGarage/BookiE/issues/1742) |
| 3 | Use one statement splitter for the editor and the policy | B4 | [#1740](https://github.com/cozyGarage/BookiE/issues/1740) |
| 4 | Upgrade duckdb and rust_xlsxwriter to the `zip ^9` releases and drop the zip advisory ignores | B3, UX | [#1058](https://github.com/cozyGarage/BookiE/issues/1058) |
| 5 | Refresh vendored redis and pick up the sqlx cancellation and decoder fixes | B3 | [#1743](https://github.com/cozyGarage/BookiE/issues/1743) |
| 6 | One liveness policy for direct sockets (keepalive, user timeout, suspend), tested with a Toxiproxy blackhole | B4, UX | candidate |
| 7 | Pin and show the PostgreSQL session time zone, decided under ADR 0007 | B3 | candidate |
| 8 | Exact-operator table per driver and value kind | B3 | candidate, covers #1708 and #1716 |
| 9 | Oldest and newest engine versions in the Forgejo driver job | UX | candidate, needs the workflow owner |
| 10 | Generic grid-write contract suite per engine, plus golden MCP transcripts for `bookie-agentd` | B3, B4 | candidate |

Candidates are not selected work. Selection belongs to the [backlog](../backlog.md) and Project #3.
