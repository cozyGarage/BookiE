# Native PostgreSQL 100,000-row profile

## Scope

This capture records one run of `SELECT generate_series(1, 100000) AS n;` in the BookiE binary extracted from the Arch package built at commit `7c2371837c4cc1b3a3fbbea014e0a7336a190ff1`. The package passed `makepkg check()` and `scripts/validate-arch-package.sh`. The extracted binary was run from the package tree; it was not installed. The installed 0.2.0 package was not used for this capture.

The query ran against the local PostgreSQL 17.11 lab connection in a 922 × 1150 window. The saved screenshot shows `done in 168 ms`, 100,000 received rows across 20 pages, and only rows 80,001–100,000 retained. The selected page is rows 80,001–85,000. Previous page is disabled, so rows 1–80,000 cannot be revisited from this result. The current full-result export action reruns the statement through a guarded cursor; it does not restore old pages to the grid.

## Process sample

`native-postgres-100k-31s.json` contains 121 samples at 250 ms intervals, starting immediately after the native Run-button click and continuing for about 30 seconds. It records `/proc/<pid>/status`, `/proc/<pid>/io` and process CPU counters.

An earlier 31-second idle sample from the same extracted process is included as `native-postgres-select1-31s.json`. It shows about 2.8 MiB of RSS variation while the `SELECT 1` result was visible and no input or query was issued. The two captures used slightly different RSS field names and sampling intervals, so compare them as context rather than as a controlled before/after benchmark.

| Metric | Before click | Peak during 30.18 s | Change |
| --- | ---: | ---: | ---: |
| RSS | 119,263,232 B (113.7 MiB) | 145,301,504 B (138.6 MiB) | +24.8 MiB |
| PSS | 104,676,352 B (99.8 MiB) | 130,714,624 B (124.6 MiB) | +24.8 MiB |
| Private dirty | 32,997,376 B (31.5 MiB) | 58,290,176 B (55.6 MiB) | +24.1 MiB |
| Process CPU time | 1.77 s | 2.77 s | +1.00 s |
| Process block reads | 96,120,832 B | 96,956,416 B | +0.8 MiB |
| Process block writes | 3,043,328 B | 3,223,552 B | +0.2 MiB |

The CPU and memory deltas cover the full 30-second sample, not just the database statement. The UI reports 168 ms, but RSS growth was gradual and reached its peak near the end of the sample. This capture therefore does not isolate query execution cost from result rendering and later UI work. `/proc/<pid>/io` block I/O does not include PostgreSQL socket traffic. This is one synthetic, narrow-row result on one local environment, not a benchmark or a process-memory ceiling.

## Artifacts

- [Screenshot](native-postgres-100k.png)
- [Raw process samples](native-postgres-100k-31s.json)
- [Idle PostgreSQL sample](native-postgres-select1-31s.json)

## Current linux-head idle capture

After the package-build tests, the extracted binary from the package built at `cc0f1cac1e6ccc08d752a23eb0b80ace8a6e1bf9` was launched on the native Hyprland desktop. It was not installed. The saved PostgreSQL connection was open in a 922 × 1150 window with an empty SQL editor and results pane. Over 31.219 seconds without input, RSS varied by 16 KiB, PSS by 28 KiB, process CPU increased by 0.01 seconds, and `/proc/<pid>/io` block-read and block-write counters did not change. This is an idle sample, separate from the query profile above; it does not measure a query or database socket traffic.

- [Current-head window screenshot](native-current-head-postgres-open.png)
- [Current-head idle process samples](native-current-head-postgres-idle-31s.json)

The same current-head process then ran `SELECT generate_series(1, 100000) AS n;` on the native desktop. The UI reported 98 ms, 100,000 received rows and 20 pages. The result retained rows 80,001–100,000. I navigated Previous to rows 80,001–85,000; Previous disabled there, while Next returned to the next retained page. This reproduces the documented forward-only retention boundary on the current source head.

This is a product gap for a completed result: the first 80,000 rows are not reachable after the query finishes, even though this narrow result has only 20 pages. The fixed four-page count, rather than a user-visible decision or a confirmed byte-budget exhaustion, is the observed boundary. The issue is tracked in [#1032](https://github.com/cozyGarage/BookiE/issues/1032). The capture demonstrates the regression surface; it does not establish the memory cost of retaining all 20 pages.

A 31.015-second sample began after the query had completed, with the grid on the newest retained page and no input. RSS stayed at 125.1 MiB, PSS varied by 13 KiB, Private Dirty decreased by 12 KiB, process CPU advanced by 0.02 seconds, and block-read/write counters did not change. This measures the retained-result idle state, not query execution or socket traffic. The earlier 30-second sample above covers a separate run that included query and UI work; the samples are not a controlled comparison.

- [Current-head oldest-retained-page screenshot](native-current-head-100k-oldest-retained.png)
- [Current-head completed-result process samples](native-current-head-postgres-100k-31s.json)

## BookiE branch verification after byte-budget retention change

The release-mode BookiE binary built from the `ux/stream-result-full-navigation` worktree ran the query against the local PostgreSQL 17.11 lab after a fresh app launch and connection. The UI reported completion in 161 ms and received 100,000 rows in 20 pages. I navigated backward from the newest page to rows 1–5,000; the grid displayed values starting at 1. This confirms that the former four-page cutoff no longer hides pages when the estimated row storage fits within the cache budget.

The process was sampled every 250 ms for 31.254 seconds, starting immediately before Run. RSS rose from 119,087,104 bytes (113.6 MiB) to a 145,596,416-byte peak (138.9 MiB), a 25.3 MiB increase. PSS rose from 91.2 MiB to a 116.5 MiB peak; private dirty memory rose from 28.1 MiB to 50.0 MiB. The peak occurred about 7.6 seconds into the sample, after the UI reported completion. Process CPU increased by 0.25 seconds; block reads increased by 16 KiB and writes by 124 KiB. The block-I/O counters do not include PostgreSQL socket traffic. This is one narrow synthetic query on one local machine, not a general benchmark or process-memory ceiling. It does not qualify results that exceed the 64 MiB estimated row-storage budget.

- [Page 1 screenshot](native-bookie-branch-100k-page1.png)
- [31-second baseline-to-peak process sample](native-bookie-branch-100k-first-run-31s.json)

## Current linux-head MySQL browse capture

The same extracted binary opened the saved Manual Lab MySQL connection and loaded the `people` table (2 rows). During a 31.238-second idle sample, RSS stayed at 124.2 MiB, PSS varied by 6 KiB, Private Dirty stayed at 36.0 MiB, CPU advanced by 0.02 seconds, and process block-read/write counters did not change. This is a small-table browse sample, not a cross-engine benchmark; the app binary was extracted from the Arch package and was not installed.

- [MySQL people table screenshot](native-current-head-mysql-people.png)
- [MySQL browse process samples](native-current-head-mysql-people-31s.json)

## Current linux-head SQL Server idle capture

With the saved Manual Lab SQL Server connection open and its schema metadata loaded, a 31.029-second sample with no BookiE input recorded RSS at 132.8 MiB, PSS at about 106.1 MiB, CPU +0.02 seconds, and no process block I/O. This is a connected idle state with no query result displayed. The extracted package binary was used, not the installed package.

- [SQL Server idle process samples](native-current-head-mssql-idle-31s.json)

## Additional native engine browse captures

Each saved profile connected and displayed its sample data in the grid. These 31-second captures were taken sequentially in the same app process after switching profiles. RSS and PSS therefore include memory retained from earlier UI and connection work; they do not isolate per-engine cost. Each interval had no BookiE input and no process block-read or block-write delta.

| Connection and view | RSS | PSS | Process CPU change | Samples |
| --- | ---: | ---: | ---: | --- |
| ClickHouse, `people` (2 rows) | 137.5 MiB | 111.3 MiB | +0.00 s | [Raw samples](native-current-head-clickhouse-people-31s.json) |
| MongoDB, `people` (2 documents) | 145.3 MiB | 119.2 MiB | +0.03 s | [Raw samples](native-current-head-mongodb-people-31s.json) |
| Redis DB 0 (5 keys) | 162.8 MiB | 135.5 MiB | +0.01 s | [Raw samples](native-current-head-redis-db0-31s.json) |
| SQLite, `people` (4 rows, mixed and null values) | 168.9 MiB | 141.7 MiB | +0.02 s | [Raw samples](native-current-head-sqlite-people-31s.json) |

The native UI sweep used the extracted package binary and the ten saved Manual Lab profiles. Each connection opened and its data view rendered successfully; no rows were changed.

| Saved profile | Native UI result |
| --- | --- |
| PostgreSQL | Ran the 100,000-row generator query; 100,000 rows were received and the newest retained rows rendered. |
| PostgreSQL Extra | Opened `people`; 2 rows rendered. |
| MySQL | Opened `people`; 2 rows rendered. |
| MySQL Extra | Opened `people`; 2 rows rendered. |
| SQL Server | Opened `dbo.people`; 2 rows rendered. `SELECT 1;` returned `1`. |
| ClickHouse | Opened `people`; 2 rows rendered. |
| MongoDB | Opened `people`; 2 documents rendered. |
| Redis | Opened DB 0; 5 keys rendered. |
| Redis DB 1 | Opened DB 1; 1 key rendered. |
| SQLite | Opened `people`; 4 rows rendered, including null and mixed-value examples. |

After switching through all ten profiles, I sampled the same process for another 31.162 seconds with SQL Server selected and an editor tab open. RSS held at 185.6 MiB, PSS at about 156.5 MiB, Private Dirty at 82.0 MiB, CPU increased by 0.02 seconds, and block-read/write counters did not change. This is the cumulative warm-process state after the full UI sweep, not an engine-isolated memory cost.

- [Post-sweep process samples](native-current-head-multi-engine-after-sweep-31s.json)

## Local storage footprint

After the ten-profile UI sweep, `du -sb` reported 125,812 bytes in `~/.config/tablepro`, 597,504 bytes in `~/.local/share/tablepro` and 4,732 bytes in `~/.cache/tablepro`. The share directory contained a 596,935-byte audit journal; the config directory contained a 5,121-byte connection file and SQLite query-history database/WAL/SHM files. The earlier capture before the full sweep was 282,903 bytes in config and 540,234 bytes in share. SQLite history files can change size as the WAL is checkpointed. These figures cover this machine's saved profiles and app data; they exclude the Docker database volumes. The current Arch package is 29,886,373 bytes compressed, the extracted application binary is 42,385,896 bytes, and package metadata reports 72.61 MiB installed size. Installation is still pending, so the new package's actual installed footprint has not been measured with pacman.

## Product finding

The installed 0.2.0 build retains four pages. PR #1568 removes that page-count cap, but its candidate still evicts old pages after the estimated row cache exceeds 64 MiB. A 100,000-row narrow result kept all 20 pages; a 100,000-row result with 2 KiB text values retained only rows 70,001–100,000. The earliest available page was rows 70,001–75,000, with Previous disabled. The completed query still reported all 100,000 rows. This makes fetched data inaccessible in the grid and remains a product gap. Large-result acceptance is tracked by [#554](https://github.com/cozyGarage/BookiE/issues/554). Evaluate a scrollable guarded cursor or disk-backed page storage while preserving query policy, audit, cancellation, timeout, value preservation and cleanup contracts.

## Native PR #1568 follow-up

On 2026-10-24, the Arch package built from PR #1568 head `748c75c0f904e6c53f41236b1a3d36668bd079b5` was extracted and run on the Omarchy Wayland desktop. The installed package remained the older `0.2.0-1`; installation of the candidate package is still pending local administrator authentication. Package SHA-256: `c23e787348daeeabeecc750735f0d7759c4567a663c391597ddee8cdf90fc2bd`. Extracted `bookie` SHA-256: `b406fd8055670abc230cfead154eb27ff39dbff7f46ca8a4b0f67a6c6c50b971`.

On PostgreSQL 17.11, `SELECT generate_series(1, 100000) AS n;` completed in 160 ms and returned 100,000 rows across 20 pages. The UI navigated from the last page back to rows 1–5,000. This confirms PR #1568 removes the installed build's four-page limit for this result. It does not test results that exceed the 64 MiB estimated in-memory page budget.

The app process was at 107,472 KiB RSS, 96,537 KiB PSS and 39,044 KiB private dirty before Run. After the query, navigating to the first page and then sitting idle for 31.72 seconds, it held 239,190,016 B RSS, 227,978,240 B PSS and 168,579,072 B private dirty. RSS and private dirty were unchanged across that idle sample; PSS varied by 8 KiB. CPU rose by 0.02 seconds. `/proc/<pid>/io` block-read and block-write counters did not change. The RSS increase includes fetched pages and GTK result rendering; this profile does not isolate those costs.

At the end of the run, `du -sb` reported 123,365 bytes in `~/.config/tablepro`, 675,264 bytes in `~/.local/share/tablepro` and 4,732 bytes in `~/.cache/tablepro`. These are whole-directory totals for the existing user profile, not per-query deltas. The screenshot records the first page after backward navigation: [native PR #1568 100,000-row result](native-pr1568-100k-page1.png).

The byte-budget boundary was tested with `SELECT n, repeat('x', 2048) AS payload FROM generate_series(1, 100000) AS n;` on the same PostgreSQL 17.11 server. It completed in 2,024 ms. The UI received 100,000 rows in 20 pages but retained only rows 70,001–100,000; Previous became disabled on rows 70,001–75,000. A [native screenshot](native-pr1568-100k-wide-budget-boundary.png) records that state. This reproduces the remaining navigation loss on the PR candidate.

After that wide query, the process held 384,832 KiB RSS, 373,697 KiB PSS and 316,180 KiB private dirty. A 42-second idle sample held RSS and private dirty steady; PSS changed by 11 KiB. Block-read/write counters stayed at 2,019,328 and 2,793,472 bytes. The user profile totals were 189,285 bytes in config, 722,490 bytes in share and 4,732 bytes in cache, unchanged during the idle sample. On a repeat run, `/proc/<pid>/stat` advanced by 230 CPU ticks over a three-second sample (2.30 CPU seconds); the UI reported 1,891 ms. RSS rose by 10,772 KiB from the preceding stable sample. The user profile totals then rose to 222,245 bytes in config and 744,941 bytes in share, with cache unchanged. These directory totals include history and audit state and are not query-result-only storage measurements.

## Native UI spot checks

On the same live desktop, clicking Split opened a second editor pane; clicking it again returned to one pane. The [split](native-pr1568-split-on.png) and [unsplit](native-pr1568-split-unsplit.png) states were captured. The sidebar toggle opened and closed the panel. At full-window width, dragging its divider widened it from about 260 px to about 400 px; hiding and showing the sidebar preserved the resized width ([capture](native-pr1568-sidebar-resized.png)). In the side-by-side layout, the pane stayed near 260 px when dragged wider, consistent with the editor's 600 px minimum-width constraint at that window size.

The isolated Xvfb GTK safety scenario `bundle_export_records_sanitized_audit_outcome` did not complete. The private D-Bus session activated the GNOME/GTK portal backends, but `xdg-desktop-portal` reported no `org.freedesktop.impl.portal.FileChooser` interface. The test could not find the app's “Export connections” dialog. This is an incomplete UI gate caused by the headless portal setup; it does not establish an application failure or a native Wayland export result.
