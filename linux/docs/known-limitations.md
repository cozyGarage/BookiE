# Known limitations of BookiE 0.2

What the application does not do, or does differently from what you might expect,
and the bugs we know about. Each entry names its ledger row. Draft of 2026-10-11,
for the maintainer; an entry is removed when the row closes.

## Data and values

- **A full export is not a consistent snapshot** (UI-13b). Rows changed while a
  large export runs can be missed or repeated, because pages are read one after
  another. Export from a quiet database when exactness matters.
- **Large results are capped and held in memory** (PERF-2, PERF-8). Results stop at
  1 million rows, 10 million cells or 64 MiB estimated size. The grid uses about
  2.5 times the memory of the result. Nothing is streamed yet.
- **Long cells are shown as a preview** (PERF-10). A table cell over 8 KiB shows a
  typed preview and its byte count; View Value fetches the whole value by primary
  key. A table without a primary key cannot refetch.
- **Hidden columns still travel** (PERF-9). A hidden column is selected and
  transferred, and actions that need the full row are refused while its value is not
  fetched.
- **A table without a primary key cannot be deleted from the grid** (TEST-14). There
  is no all-column fallback, by design.
- **ClickHouse ambiguous local times:** a `DateTime64` that falls in a repeated or
  missing daylight-saving hour is shown as undecodable and is never turned into a
  literal, because the instant cannot be known.
- **SQLite real numbers:** negative zero and NaN cannot be bound as parameters.
- **SQLite JSON aggregate output** can differ across system SQLite versions (TEST-25).
- **MongoDB:** browse pages are separate observations, not one point-in-time view
  (TEST-12); a column with mixed types is read-only; large collections are slow to
  browse (PERF-3).
- **MySQL failed batches:** rollback of a failed batch is verified for InnoDB; other
  storage engines (MyISAM, MEMORY) do not roll back, and the application cannot
  change that (B4-11).

## Editing and connections

- **SSH jump chains cannot be edited in the form** (UI-1b). They connect and run,
  but the form refuses to edit them until per-hop secrets, transport identity and the
  bundle format all land and pass together.
- **Reusable SSH profiles cannot be edited** (UI-26).
- **SQL Server with Kerberos** is tested by hand only, because there is no
  deterministic KDC fixture yet (B4-21).
- **DuckDB read-only flat files** need Linux and a writable directory on the same
  filesystem as the file (the application pins the file through a private link there).
  A read-only in-memory DuckDB connection is refused.

## Interface

- No code folding, vim mode, multi-cursor or split panes (UI-22); no preview tabs
  (UI-17b); history is a dialog (UI-18).
- The PostgreSQL tree lacks materialized views, routines, triggers, sequences,
  extensions and roles (UI-23).
- No server messages (NOTICE, PRINT) and no timing breakdown; timing shows elapsed
  time only (UI-20, PERF-4).
- No foreign key picker, enum or set pickers, or paste of tab-separated rows (UI-19).
- No database snapshots or restore (SNAP-1; planned for 0.2.x).
- Accessibility with Orca, keyboard-only order and high contrast has not been
  verified (DOC-3).

## Platforms and packaging

- Native Wayland install, upgrade and rollback are not yet verified on a real
  desktop (PKG-1, PKG-2). Xvfb results do not count.
- The Flatpak build is not verified and is not offered (PKG-4, PKG-8).
- Windows and macOS are not supported; the application is built for Linux with GTK 4
  and libadwaita.

## Known bugs

- **Startup layout abort** (TEST-31): in the automated test setup the application
  sometimes aborts at startup when it restores saved state, after GTK logs a
  widget being given a width near minus two billion. It is not yet known whether this
  can happen outside the test setup, where GTK only warns.

## Test and CI limits

- The installed GTK suite does not pass reliably on a native Ubuntu 24.04 runner
  (TEST-28); it passes in the Ubuntu container.
- About 68 percent of the policy crate's and 81 percent of the core crate's
  mutations are caught by their own tests; the survivors are listed in the
  [mutation reports](evidence/test-quality-2026-10-11/policy-mutants.md) (TEST-32).
