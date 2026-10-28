# Source-build manual run, 2026-10-28

This record covers a driven run of the app built from source. It is not an installed-package acceptance run and does not replace the [installed Wayland records](installed-wayland-0.2.0-2026-10-22/README.md).

## Identity

- Source: `bb9202644` on `linux` (clean checkout, debug build with `cargo build -p tablepro-app`) for most checks. A last smoke run used a release build of `9b3dff901`, installed with `cargo install --locked --path apps/linux/app` into a private prefix (78 MB binary).
- Session: native Wayland on Hyprland 0.56.2, display scale 1, dark appearance, window tiled to 922 px wide.
- Toolkit: GTK 4.24.1, libadwaita 1.10.0, GtkSourceView 5.22.0, Linux 7.2.8.
- Profile: private `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME`, `GSETTINGS_BACKEND=memory`, so the user's saved connections and settings were not touched.
- Databases: MySQL behind a bastion (`tests/fixtures/driver-tls`), Redis 7.4.10, ClickHouse 24.8, SQL Server 2022, MySQL 8.4.11 (`mysql:8.4`), MongoDB 7.0.45 (`mongo:7`), PostgreSQL 18.6 (`postgres:18-alpine`, trust authentication, schemas `public`, `shop` and `Mixed Case`) and a SQLite 3.54.0 file with four tables and one view.
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
| High contrast and icon | With libadwaita's high-contrast debug switch the editor and grid stayed readable; with the shipped icon on the data path About showed the BookiE artwork |
| Built-in SSH tunnel | MySQL 8.4 behind a bastion container (the repository's driver-tls fixture): host-key trust prompt, tunnel with TLS Verify Full (`TLS_AES_256_GCM_SHA384`), "Connection lost — reconnecting" when the bastion stopped, changed host key refused |
| SSH agent and system OpenSSH | Run with a scratch `HOME` and a private agent so the real `~/.ssh` was untouched: agent authentication works, a stopped agent gives a clear error, system OpenSSH shows the trust dialog, Cancel writes nothing and Trust connects |
| Backup progress and Cancel | A 1.3 GB database kept `pg_dump` running long enough to see the progress window (spinner and Cancel only); Cancel stopped it and left no file |
| Arch package, unpacked | `scripts/build-arch-rc.sh` (release profile, `--nodeps`) built `bookie-0.2.0-1-x86_64.pkg.tar.zst` (SHA-256 `1a9aebe7…169f8`, 35 MB) from `893e83a1f`. `desktop-file-validate` and `appstreamcli validate --no-net` passed, the GSettings schema compiled, and the unpacked binary started, loaded its schema, connected to SQLite and showed the packaged icon in About. It was not installed with `pacman` |
| Installed Arch package | The package rebuilt from `cbe0a5816` (`bookie-0.2.0-1-x86_64.pkg.tar.zst`, SHA-256 `909ffbc0…efe2`) was installed with `pacman -U` and `/usr/bin/bookie` was driven with a private profile: it started, connected to SQLite, and About showed the installed icon and version 0.2.0. The desktop entry resolved to `BookiE`, `bookie %U`, icon `com.tablepro.linux`, and the launcher listed it under Apps |
| Input method | fcitx5 5.1.23 was running with only `keyboard-us` configured; its built-in Unicode addon gave a real preedit and candidate popup. Cell editor and editor tabs accepted commits; closing was refused during a preedit; the first commit after a restore landed in a hidden tab (#1943). No language engine (Pinyin, Anthy, Unikey) is installed, so composition of real language text was not run |
| CSV import, second pass (installed package) | A duplicate-key row stopped and rolled back the import with a row count; duplicate headers shown as `a` and `a [1]`; a headerless file refuses a saved mapping; Table from CSV inferred editable types and created the table; import refused on a read-only connection |
| ENUM and SET pickers (installed package) | MySQL 8.4 ENUM and SET pickers write declared order, the empty member and the empty set correctly; PostgreSQL enum labels come in declared order; the SET popover clips its labels and Apply (#1945) |
| SQLite Structure and tab menu (installed package) | Add Foreign Key is greyed with the right reason on an existing table; a disabled Comment row is shown on SQLite (#1946); the tab context menu entries are greyed and inert (#1947) |
| Pending-edit teardown and MySQL comments (installed package) | Disconnect, Discard and disconnect, Ctrl+Q and window close all prompt with a pending cell edit and Cancel keeps the draft; MySQL column comments edit and clear correctly |
| Editor, second pass (installed package) | Lone BEGIN refusal, empty Explain message, close with an open transaction and Roll Back and Close, dimmed result while a query runs, tick and cross gutter marks |
| Editor files and saved queries (installed package) | A symlinked file saves through to its target; an oversized file is refused; Escape closes Explain; Ctrl+Shift+D is claimed by GTK's inspector prompt (#1948) |
| Connection bundles (installed package) | Plain export carries no secrets; passphrase export is encrypted; wrong passphrase and tampered file give one message; import plan lists dispositions and honours unticking; an empty profile has no import entry (#1949) |
| Connection list (installed package) | 3 s connect timeout fails in 3 s with a clear message; Duplicate gives a distinct name and an empty-password form; Group and tags and the colour swatch work |
| Session refusals (installed package) | MySQL `SET autocommit=0` and `XA START` refused on a shared connection with a wrong-looking message (#1952); ClickHouse Session explains it is unavailable |
| Hide schema and failed structure save (installed package) | Hiding a PostgreSQL schema removes it from the tree and search, and it is restorable from the Hidden schemas menu; a NOT NULL column added to a populated table fails with a clear rollback message |
| Host down with an open session (installed package) | Ctrl+Q with an open Session transaction and the database stopped leaves the confirmation dialog stuck with no cleanup-timeout dialog (#1953); an idle session closes the window normally |
| Session cancel and timeout (installed package) | Cancel stops a server query inside a Session; a timeout inside a Session transaction leaves the transaction open and says so |
| Large export to an existing file (installed package) | Replace prompt shown; the destination was untouched while 1M rows were written; the cancel path was not exercised because the 4M-row export finished first; a 4M-row editor result stops with "could not keep the complete result in temporary storage" while the guarded-cursor export still completed |
| Filter by This Value (installed package) | Text value, NULL, empty text and a BLOB behave as the checklist says |
| Long values (installed package) | Copy value copies the full text; BLOB View Value is capped at 16 KiB with the total shown; long unbroken text wraps without hyphens |
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
- [#1925](https://github.com/cozyGarage/BookiE/issues/1925): MySQL `SHOW STATUS LIKE …` is held for approval as an unknown write.
- [#1932](https://github.com/cozyGarage/BookiE/issues/1932): the backup progress window shows only a spinner and Cancel, with no text.
- [#1938](https://github.com/cozyGarage/BookiE/issues/1938): `bookie --version` starts the app instead of printing the version. A first attempt to read the version this way started the app against the real profile for under a second; no real file was modified.
- [#1943](https://github.com/cozyGarage/BookiE/issues/1943): the first IME commit after restoring a multi-tab workspace landed in a background tab (observed once).
- [#1945](https://github.com/cozyGarage/BookiE/issues/1945): the SET picker popover clips its labels and Apply (regression of #657).
- [#1946](https://github.com/cozyGarage/BookiE/issues/1946): SQLite Structure shows a disabled Comment row.
- [#1947](https://github.com/cozyGarage/BookiE/issues/1947): the tab context menu entries are greyed out and do nothing.
- [#1948](https://github.com/cozyGarage/BookiE/issues/1948): Ctrl+Shift+D opens the GTK Inspector prompt instead of the saved-queries list.
- [#1949](https://github.com/cozyGarage/BookiE/issues/1949): a profile with no saved connections has no way to import a bundle.
- [#1952](https://github.com/cozyGarage/BookiE/issues/1952): refusing `SET autocommit=0` or `XA START` shows the BEGIN/COMMIT/ROLLBACK message.
- [#1953](https://github.com/cozyGarage/BookiE/issues/1953): closing the window with an open session transaction while the database is down leaves the confirmation dialog stuck.
- [#1966](https://github.com/cozyGarage/BookiE/issues/1966): creating a connection with the same endpoint, database and user as a saved one silently overwrites that saved connection.
- Re-confirmed on an existing issue: an editor INSERT that wrote a row shows "No rows returned" ([#1486](https://github.com/cozyGarage/BookiE/issues/1486)).
- The editor toolbar wrapped onto three rows at 922 px and the bottom bar kept its unlabelled icon between Results and Errors; both are already tracked in [#1862](https://github.com/cozyGarage/BookiE/issues/1862) and [#1864](https://github.com/cozyGarage/BookiE/issues/1864).

## Coverage

Counted from the four checklists at the head of this run. "With a 2026-10-28 result" means the item carries a note from this run, whether it passed, failed or was inconclusive. "Ticked" means the item was fully verified and is ticked.

| Checklist | Items | Ticked | With a 2026-10-28 result | Without one |
|---|---|---|---|---|
| Catalog, sidebar and application shell | 38 | 7 | 20 | 18 |
| Connections and SSH | 44 | 15 | 29 | 15 |
| SQL editor and sessions | 62 | 30 | 34 | 28 |
| Grid, filters and data transfer | 76 | 16 | 43 | 33 |
| **All four** | 220 | 68 | 126 | 94 |

Why the rest are open:

- Visual audits that need a measured or manual pass (contrast ratios, keyboard-only walks, large fonts, wide and narrow widths, light and dark side by side).
- Per-engine variants of checks already run on another engine (MySQL, SQL Server, ClickHouse, MongoDB, Redis).
- Fixtures not built: a Session query that outlives its cancel, a language input method, DuckDB, a MariaDB server, a Kerberos realm.
- Blocked by a filed defect: the tab context menu ([#1947](https://github.com/cozyGarage/BookiE/issues/1947)).
- Package formats other than the installed Arch package: `.deb`, Flatpak and AppImage were not built.

## Not run

Connections items other than the Production marker, the 2-second timeout, the connection form and the built-in SSH host-key and tunnel-loss checks; ProxyJump and saved-password cases for system OpenSSH; the remaining ClickHouse, SQL Server, Redis, MySQL and MongoDB items, a system high-contrast setting, composition with a language input method (only fcitx5's Unicode addon was available), and the other distribution formats: no `.deb`, Flatpak or AppImage was built. The Arch package was installed with `pacman -U` by the user. The release build installed with `cargo install` into a private prefix connected to PostgreSQL, browsed a table, ran a query and opened About, but it carries no icon, desktop or schema files.

## Cleanup

The app, the PostgreSQL container, `ydotoold` and the temporary worktree started for this run were stopped and removed afterwards.
