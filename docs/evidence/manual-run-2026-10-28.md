# Source-build manual run, 2026-10-28

This record covers a driven run of the app built from source. It is not an installed-package acceptance run and does not replace the [installed Wayland records](installed-wayland-0.2.0-2026-10-22/README.md).

## Identity

- Source: `bb9202644` on `linux` (clean checkout, debug build with `cargo build -p tablepro-app`) for most checks. A last smoke run used a release build of `9b3dff901`, installed with `cargo install --locked --path apps/linux/app` into a private prefix (78 MB binary).
- Session: native Wayland on Hyprland 0.56.2, display scale 1, dark appearance, window tiled to 922 px wide.
- Toolkit: GTK 4.24.1, libadwaita 1.10.0, GtkSourceView 5.22.0, Linux 7.2.8.
- Profile: private `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME`, `GSETTINGS_BACKEND=memory`, so the user's saved connections and settings were not touched.
- Databases: Redis 7.4.10, ClickHouse 24.8, SQL Server 2022, MySQL 8.4.11 (`mysql:8.4`), MongoDB 7.0.45 (`mongo:7`), PostgreSQL 18.6 (`postgres:18-alpine`, trust authentication, schemas `public`, `shop` and `Mixed Case`) and a SQLite 3.54.0 file with four tables and one view.
- Driving: pointer and keyboard input through `ydotool`, window geometry and screenshots through `hyprctl` and `grim`, text read from screenshots with `tesseract`, server state checked with `psql`. Screenshots are not committed.

## Results

The run was done in two sessions on the same build, one for the first six rows and one for the rest. The per-item results are recorded beside each item in the [checklists](manual-verification-0.2-features.md#areas). In short:

| Area | Result |
|---|---|
| Catalog window | Passed: all seven kinds list objects and five counts match `psql`; SQLite says the engine does not list the kind |
| Sidebar tree | Groups, counts, collapsing and search passed; the search-opened group keeps a collapsed chevron ([#1907](https://github.com/cozyGarage/BookiE/issues/1907)); SQLite does not list its view ([#1908](https://github.com/cozyGarage/BookiE/issues/1908)) |
| Production marker | Passed: red bar, both badges, `[Production]` title prefix, gone on a Local connection |
| Per-connection timeout | A 2-second timeout stopped `SELECT pg_sleep(5)` with the expected message; elapsed time was not measured |
| Editor Session | Temp table persisted, transaction label and server state matched, Roll back & close left no open transaction |
| Explain tree | Passed: nested steps, cost bars, filter and condition lines |
| Export dialog | CSV options hide for JSON; SQL options appear only for SQL; Excel absent for an unknown total, as already recorded |
| Editor files | Passed: Ctrl+O, Ctrl+S with and without a file, Ctrl+Shift+S, changed-on-disk dialog (Cancel, Overwrite), close prompt (Save), restart restore with unsaved text, deleted-file restore with its toast |
| PostgreSQL backup and restore | Passed: a 31 MB dump, restore after approval returned a changed row, restore refused on a read-only connection before any chooser |
| CSV import | Passed on SQLite: semicolon and comma files detected, prefill, a changed mapping, a headerless file, rows written to the right columns |
| MySQL 8.4 | Both backup actions explain they are PostgreSQL only; Catalog says the engine does not list the kind; Session kept a user variable; a passwordless root was refused ([#1861](https://github.com/cozyGarage/BookiE/issues/1861) reproduced); the view is listed under Tables |
| MongoDB 7 | Collection browse works; SQL export absent; Excel offered for a known total |
| Installed release build | Started, connected, browsed, ran a query and showed version 0.2.0 in About; no icon files, so the launcher icon is unchecked |
| Connection form | Driver list and per-driver defaults correct; MySQL Unix socket mode shows the socket path and hides host and port; Escape dismisses without saving |
| Redis 7.4 | Keys, type, TTL and value shown for db0; SQL export absent; the sidebar lists db0 to db15 under a Tables group with a New Table button |
| ClickHouse 24.8 | Connected; Structure showed the comment greyed out |
| SQL Server 2022 | A missing password gave "Username or password is wrong."; after entering it the connection worked and a column comment save wrote `MS_Description` after approval |
| Editor files, second pass | Close prompt Cancel and Discard and Save As in the changed-on-disk dialog passed |
| Restore refresh | A browse tab open on a changed table refreshed to the restored value |
| Light appearance | Applied at once and survived a restart; Production header readable in light |

## Defects filed

- [#1905](https://github.com/cozyGarage/BookiE/issues/1905): the connection list popover is clipped at the left edge of a narrow window.
- [#1906](https://github.com/cozyGarage/BookiE/issues/1906): a completion chip stays on screen after a query runs and floats over the Export dialog.
- [#1907](https://github.com/cozyGarage/BookiE/issues/1907): a search-opened sidebar group keeps its collapsed chevron.
- [#1908](https://github.com/cozyGarage/BookiE/issues/1908): SQLite views are not listed in the sidebar.
- [#1910](https://github.com/cozyGarage/BookiE/issues/1910): "Run again with a 8s timeout".
- [#1916](https://github.com/cozyGarage/BookiE/issues/1916): MongoDB nested numbers show as canonical extended JSON while top-level numbers are plain.
- [#1917](https://github.com/cozyGarage/BookiE/issues/1917): MongoDB fields a document lacks show `<undecodable missing BSON field>`.
- Reproduced on an existing issue: a passwordless MySQL root is refused with "using password: YES" ([#1861](https://github.com/cozyGarage/BookiE/issues/1861)); MySQL lists views under Tables while PostgreSQL has a Views group and SQLite omits them ([#1908](https://github.com/cozyGarage/BookiE/issues/1908)).
- [#1919](https://github.com/cozyGarage/BookiE/issues/1919): the approval prompt shows an empty `tables:` line when no targets were detected.
- [#1921](https://github.com/cozyGarage/BookiE/issues/1921): the sidebar cannot be refreshed after tables are created outside BookiE.
- Re-confirmed on an existing issue: an editor INSERT that wrote a row shows "No rows returned" ([#1486](https://github.com/cozyGarage/BookiE/issues/1486)).
- The editor toolbar wrapped onto three rows at 922 px and the bottom bar kept its unlabelled icon between Results and Errors; both are already tracked in [#1862](https://github.com/cozyGarage/BookiE/issues/1862) and [#1864](https://github.com/cozyGarage/BookiE/issues/1864).

## Not run

Connections and SSH items other than the Production marker and the 2-second timeout, the remaining ClickHouse, SQL Server, Redis, MySQL and MongoDB items, high contrast, input-method editing, the backup progress window, and a distribution package (`.deb`, Arch, Flatpak or AppImage): no package was installed. The release build installed with `cargo install` into a private prefix connected to PostgreSQL, browsed a table, ran a query and opened About, but it carries no icon, desktop or schema files.

## Cleanup

The app, the PostgreSQL container, `ydotoold` and the temporary worktree started for this run were stopped and removed afterwards.
