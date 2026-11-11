# Changelog: BookiE

## [Unreleased]

## [0.2.7] - 2026-11-11

### Added

- The New Table tab now lets you pick each column's collation from the server's list, on engines that provide one. (#2862)
- The New Table tab on MySQL and MariaDB can now set the table's default collation from the server's list. (#2865)

### Changed

- On SQL Server, a query that reaches another server through a linked server name (four-part names) now asks for approval first. (#2859)
- GTK grid: columns start at widths that suit their type, and the last column fills the view. (#2848)
- The result grid right-aligns numeric columns, with their NULLs. (#2846)
- GPUI retains live query results in a compact row store to reduce duplicate decoded-result memory. (#2844)
- Large database sidebars remain responsive while the full catalog loads in batches. (#2871)
- The AppImage requires glibc 2.43 or newer. (#2875)
- Grid column headers align with their cell values. (#2874)

### Fixed

- Result pages finishing after their rows disappear no longer remain unnecessarily loaded. (#2751)
- A delayed row delete no longer discards a different unsaved draft after the grid changes. (#2755)
- A grid menu no longer redirects a row delete to another row if the cell changes while the menu is open. (#2756)
- Set NULL no longer follows a changed grid-cell binding to another row. (#2758)
- Duplicating a row from its menu no longer targets a different row after the grid changes. (#2764)
- The full-value editor no longer opens a different row after the grid changes. (#2774)
- Applying a full-value edit no longer changes a replacement draft row. (#2777)
- A stale cell menu no longer starts editing a replacement row. (#2778)
- A stale cell menu no longer filters using a replacement row. (#2782)
- Cell menu actions no longer use a different row after the grid changes. (#2786)
- Large JSON cell previews avoid scanning the full value. (#2798)
- The AppImage now uses libadwaita's own styling, with proper spacing and text hierarchy in dialogs and Preferences. (#2807)
- Closing an edited table keeps keyboard focus in the workspace. (#2851)
- Confirming an edit on a cell that holds an empty string or empty binary value no longer turns it into NULL. (#2843)
- Contains, Starts with and Ends with filters on Oracle now treat percent signs, underscores and backslashes as literal text. (#2847)
- Cancelling a CSV import into SQLite while a batch is running now records a cancelled import with the rows already written and no longer blocks later writes on the connection. (#2853)
- In a narrow window, picking a table closes the sidebar. (#2854)
- The result grid and query results show NULL the same way, and expression columns no longer show a type label. (#2842)
- The SQL editor's status text keeps a fixed width, so the Run button no longer moves when a statement finishes. (#2857)
- The Arch package now declares the OpenSSL 4 libraries it links against. (#2855)
- A server that never answers the version request no longer holds a connection attempt open in the connect dialog. (#2860)
- The column collation picker now works on MariaDB servers that list a collation without a character set. (#2864)
- Inactive streamed result views release loaded pages and restore them from the saved result when shown again. (#2867)
- CSV preview updates no longer block the import dialog on large files. (#2878)
- Connection names render as plain text in history filters. (#2880)

## [0.2.6] - 2026-11-10

### Added

- DataGrip imports keep each data source's group. (#2461)
- Navicat and RESP.app connection exports import as drafts. (#2462)
- Sidebar: functions, procedures and triggers are listed under the Catalog group. (#2464)
- Keyboard shortcuts to move focus between the SQL editor and its active result grid. (#2506)
- Edit SQL editor results that read one table and include its primary key. (#2507)
- Choose how the data browser shows dates and timestamps. (#2509)
- Create and drop databases and schemas from the sidebar and database switcher. (#2510)
- Ctrl+Shift+E exports results from the front tab. (#2515)
- The full-value editor can mark spaces, tabs and line breaks. (#2516)
- Result and browse grids keep a sticky row-number gutter while columns scroll sideways. (#2519)
- Nested folders for saved connections on the welcome list. (#2521)
- Result grids can Find with match case, whole words, regular expressions and a column scope, and Replace only in editable text cells on the loaded page. (#2522)
- Result grids can sort by multiple local column keys from the header, with clear. (#2524)
- Toggle and jump SQL editor line bookmarks. (#2528)
- Highlight Rules editor to add, edit, and remove per-table grid colouring. (#2544)
- Open several database connections in one BookiE window and switch between them. (#2565)
- An experimental, read-only Oracle Database driver (SELECT queries and browsing over plain TCP). (#2634)
- SSH tunnels can listen on a fixed local port chosen in the connection form. (#2657)
- SQL Server connections can require TDS 8.0 strict encryption. (#2666)
- Structure lets you pick a column collation from the list the server supports. (#2686)
- PostgreSQL sequences show their current value in the catalog and can be restarted at a chosen value. (#2693)
- View Value shows a preview of PNG, JPEG, GIF and WebP images stored in binary cells. (#2698)
- An experimental read-only Cassandra and ScyllaDB driver for browsing keyspaces and running SELECT queries. (#2705)

### Changed

- Large byte values display a compact byte-count summary in result cells. (#2480)
- Hidden PostgreSQL schemas are excluded from GUI catalog listings for that connection. (#2552)
- Copying binary values uses fewer temporary allocations. (#2557)
- Editing SQL results preserves source-column ownership when columns are aliased. (#2562)
- ClickHouse grid editing is disabled when table metadata cannot establish a unique row identity. (#2568)
- Case-insensitive Replace All handles cells with many matches without rescanning the cell for each match. (#2601)
- PostgreSQL catalogs stay responsive when many schemas are hidden. (#2603)
- Browse now explains that secondary sort keys apply only to the loaded page. (#2633)
- Row highlight matches are reused while a grid renders. (#2644)

### Fixed

- Result comparison keeps key choices and actions reachable when results have many columns. (#2440)
- Dismissing a dirty-tab close confirmation no longer crashes the application. (#2446)
- Dismissing history deletion confirmations no longer crashes the application. (#2446)
- A restricted PostgreSQL role no longer sees tables and views it cannot reach in the sidebar, completion and tools. (#2413)
- Dismissing connection organization and URL import dialogs no longer crashes the application. (#2467)
- Failed saved result pages can be retried without changing pages. (#2456)
- Dismissing a referenced-row chooser no longer crashes the application. (#2466)
- Dismissing a connection failure no longer crashes the application. (#2463)
- A restricted PostgreSQL role no longer sees routines, triggers, sequences, types or grants it cannot reach. (#2414)
- Importing a DBeaver connection list keeps nested folders, written as a path like prod/finance, as one connection group, and an odd folder definition no longer blocks the import. (#2474)
- Toasts, banners, History and other rows, group names and error pages now show text containing &, < or > exactly as written instead of blank or garbled. (#2470)
- Dismissing connection confirmations no longer aborts the app. (#2477)
- Settings saves refuse temporary-file collisions without overwriting unrelated files. (#2486)
- Refused disabled cleartext MySQL authentication before sending a password. (#2487)
- Explain refused cleartext MySQL authentication with guidance for configuring secure authentication. (#2489)
- Copy Value keeps the full hex form of binary cells instead of a byte-count placeholder. (#2550)
- New editor tabs use the currently selected connection. (#2573)
- Grid Replace All stays responsive, can be cancelled, and stages edits only after the loaded rows are revalidated. (#2589)
- Row insert and duplicate actions no longer discard edits while a result save is in progress. (#2607)
- Explain the highlight pattern limit and prevent saving new patterns that exceed it. (#2632)
- Show when a saved highlight pattern is too long to apply. (#2638)
- Oracle connections now apply the configured connection timeout through the native client. (#2639)
- Oracle stored functions in queries require policy approval. (#2641)
- SSH tunnels through system OpenSSH older than 8.5 no longer fail on the weak-algorithm policy. (#2653)
- Cancelling a connection attempt interrupts catalog loading. (#2685)
- Closing the window with an open session transaction keeps its confirmation responsive. (#2699)
- A read-only ClickHouse connection now refuses writes at the driver, not only through the policy guard. (#2701)
- The experimental Oracle driver now refuses a locking SELECT … FOR UPDATE even when comments sit between the keywords. (#2708)
- Row highlight rules with complicated LIKE patterns no longer stall grid rendering. (#2628)

## [0.2.5] - 2026-11-04

### Added

- SQL completion now suggests names defined in a WITH clause of the current statement. (#2326)
- Copy a grid row as a WHERE clause from the Copy as menu. (#2328)
- Import connections from a DBeaver data-sources.json file, without passwords. (#2336)
- BookiE asks before quitting while an import or backup is still running. (#2340)
- Copy a schema diagram as Mermaid or DBML text. (#2341)
- CSV import can skip rows the database rejects and lists each skipped row when it finishes. (#2343)
- Save SQL snippets and insert them at the cursor from the editor toolbar. (#2344)
- SQL completion offers the columns of a WITH clause and of a derived table in the same statement. (#2346)
- Right-click a table to open a starter JOIN query built from its foreign keys. (#2347)
- Import connections reads an Another Redis Desktop Manager export as Redis drafts without passwords. (#2402)
- Import connections reads a DataGrip dataSources.xml as drafts without passwords. (#2404)
- Importing a DBeaver connection list keeps its folders as connection groups. (#2447)

### Changed

- Importing a connection URL for a remote host now saves it as Production with verified TLS instead of Local without TLS. (#2337)
- Copy as INSERT, UPDATE, DELETE and WHERE Clause now use all selected rows. (#2330)
- Large parsed JSON previews stop counting once their size exceeds the sample budget. (#2415)
- BookiE asks before quitting while a result export is still running. (#2400)
- BookiE asks before closing a window that is running an import, backup or export, even when other windows are open. (#2408)

### Removed

- Releases no longer include a Debian package or a Flatpak bundle; use the Arch package or the AppImage. (#2342, #2395)

### Fixed

- SQL generated for a negative-zero float on PostgreSQL now keeps the sign. (#2351)
- Exporting query history as SQL no longer swallows the statement terminator when a query ends in a line comment. (#2352)
- MCP `execute_write` preview on MySQL no longer runs DDL and other implicitly committing statements. (#2329)
- Agents can no longer read host files through DuckDB table functions or quoted file paths. (#2331)
- Agents can no longer read DuckDB log, variable and prepared-statement catalogs, or the host environment and credential settings, as plain reads. (#2332)
- Undoing an edit in a new row no longer replaces an explicit NULL with the column default. (#2333)
- MCP write previews on MySQL now refuse a non-transactional table in another database. (#2334)
- A slow View Value or cell edit no longer opens on a different row after you sort, filter or change page. (#2339)
- Pending browse edits are preserved when an inactive save reports completion. (#2313)
- Undoing an edit on a new row no longer replaces a NULL you typed with the column default. (#2325)
- A database connection through an SSH tunnel now fails with a timeout instead of hanging when the SSH server never answers. (#2360)
- ClickHouse connections to an IPv6 address no longer fail with an invalid address. (#2368)
- Browsing Redis keys no longer fails the whole page when a hash, list, set or sorted set contains a member that is not valid text. (#2370)
- A failure to save an audit record no longer leaves a half-recorded operation that can block later writes on that connection. (#2354)
- An MCP client over stdio can now cancel a tool call and keeps getting answers to ping and tools/list while a slow call or an approval is pending. (#2362)
- A terminal approval prompt in bookie-agentd no longer leaves a read running after it times out, and a late answer can no longer approve the next request. (#2364)
- SQL completion remains available in large scripts without planning on the GTK thread. (#2379)
- A late save result no longer clears newer table edits. (#2380)
- A browse save without an active connection no longer leaves a close request pending. (#2380)
- Connection imports show skipped unsupported connections and file parsing failures. (#2444)
- Import dialogs safely dismiss and display connection names literally. (#2444)

### Security

- Issuing an MCP token to a file no longer follows a symlink or writes to something that is not a regular file. (#2348)
- Revoking an MCP token that does not exist now reports an error instead of appearing to succeed. (#2349)
- SSH tunnels now refuse legacy SHA-1 ssh-rsa host keys and other weak algorithms, and the connect error names the refused class. (#2451)
- BookiE now refuses to send a MySQL or MariaDB password over an unencrypted TCP connection when the server asks for an RSA password exchange, and tells you to turn on TLS or use a local socket. (#2450)

## [0.2.4] - 2026-11-03

### Added

- Completed streamed results can be exported without rerunning the query. (#2284)
- The welcome screen offers recently opened connections. (#2284)
- Find in all rows, Show matching rows and Compare results work across a whole streamed result. (#2292)
- Show all rows equal or not equal to a cell value, and changed-cell highlighting in keyed Compare. (#2292)

### Changed

- Editable table browsing uses less memory. (#2279)
- Large CSV exports use less temporary memory while preserving the existing output. (#2283)
- Large database sidebars use less memory while loading their tables. (#2285)
- Streamed query results show progress and support continuous navigation after completion. (#2284)
- The About credits no longer list TablePro as an upstream project. (#2290)
- The result header wraps instead of stacking controls on narrow windows. (#2292)
- Large JSON string cells preview with less processing. (#2289)

### Fixed

- Continuous results keep both visible pages readable when scrolling across wide-page boundaries. (#2286)
- Result-page errors clear when scrolling away from the failed page. (#2287)
- Reject malformed workbook inputs without parser panic or unbounded allocation. (#2299)
- Streamed result views keep their sort and selection when results are pinned or filtered. (#2302)
- F2 and Enter edit the clicked table cell. (#2288)
- Find in all rows can be repeated with Enter during a scan, and jumps work on a locally filtered page. (#2306)
- Clicking inside a cell that is being edited no longer ends the edit. (#2308)

## [0.2.3] - 2026-11-02

### Added

- Filter by this value and Exclude this value from the cell context menu. (#2220)
- Zoom the focused result grid font with Ctrl+=, Ctrl+-, and Ctrl+0. (#2223)
- The DuckDB driver is included in the released packages. (#2246)
- The SSH section can name the one agent key to offer when the agent holds many keys. (#2254)

### Changed

- Wide editable result grids use less memory. (#2186)
- Large SQL scripts use less memory during completion and avoid scanning token history unrelated to the cursor. (#2187)
- Completed streamed results use less memory while pinned. (#2191)
- Large database sidebars construct row context menus only when requested. (#2189)
- Filtering and sorting large loaded results now sorts only matching rows and uses less temporary memory. (#2200)
- Wide result grids and large query loads use less memory. (#2260, #2275)
- Large sidebar searches do less transient work. (#2263)
- Streamed result row and page counts use thousands separators like the rest of the grid chrome. (#2234)
- BookiE now uses bookie names everywhere and copies settings, connections and saved credentials from an earlier tablepro install on first start. (#2239)
- The About dialog no longer describes BookiE as based on TablePro and credits the TablePro authors for adopted code. (#2245)
- PostgreSQL connections keep the server or role time zone, so date and time functions match psql. (#2253)
- ClickHouse rows that share a key with another loaded row are shown as not editable, with a tooltip explaining why. (#2255)

### Fixed

- MySQL Explain Text tab shows column names above the plan rows. (#2181)
- Restore failure alerts show the pg_restore detail without an Internal driver error prefix. (#2183)
- Row inspector values containing markup characters display as literal database text. (#2184)
- SQL editor completion closes when a query starts. (#2227)
- SQL editor lines no longer clip the tops of highlighted keywords. (#2222)
- Opening a .sql file from the desktop launches it in the SQL editor. (#2226)
- A failed PostgreSQL backup no longer deletes another process's partial dump file if the temporary path already exists. (#2228)
- Statement gutter success and error marks keep at least 3:1 contrast in light, dark and high-contrast themes. (#2230)
- Streamed results keep the page you selected when the query finishes. (#2240)
- Redis TLS connections no longer report handshake errors when a connection times out. (#2241)
- Development builds no longer share MCP tokens or SSH host keys with an installed release. (#2243)
- The Standard NULL preference label matches the grid marker. (#2234)
- DuckDB views appear under Views in the sidebar. (#2236)
- DuckDB result grids show DuckDB type names such as INTEGER and VARCHAR. (#2251)
- A very large DuckDB value no longer causes a large temporary allocation before the result limit applies. (#2252)
- Long values in the row inspector wrap without inserting display-only hyphens. (#2256)
- Leaving a grid cell keeps the word still being composed by the input method. (#2257)

## [0.2.2] - 2026-11-01

### Added

- Revisit earlier pages of a large PostgreSQL result without rerunning the query. (#1660)
- Open the exact audit record from a failed SQL statement. (#1670)
- The sidebar has a Collapse all / Expand all button, and PostgreSQL connections get a Catalog group that opens the Catalog dialog on a chosen object kind. (#1877)
- AppStream screenshots for Linux software stores and desktop file associations for SQL scripts and database URI schemes. (#1981)
- Copy as Text Table, JSON Lines, and Values List clipboard options to data grid cell menu. (#1982)
- ClickHouse multi-dimensional array and nested structure decoding and export preservation. (#1986)
- Show Referencing Rows in the table browse grid context menu navigates to matching rows in tables that reference the current row. (#1984)
- Attach additional SQLite database files to a connection for cross-schema querying and schema browsing. (#1985)
- Show ER Diagram on a schema or table in the sidebar draws the schema's tables and foreign keys, with pan and zoom. (#1880)
- SQL Server multi-schema trigger and object catalog inspection and native batch execution. (#1988)
- SSH connections through the built-in client now use OpenSSH certificates held by the SSH agent. (#1937)
- A saved SSH connection can name the one agent key to use, so an agent holding many keys no longer trips the server's attempt limit (settable in the saved connection file; no form control yet). (#1936)
- Query history keeps at most 100,000 unpinned entries; pinned entries are always kept. (#2072)

### Changed

- Transaction age labels show translated minute and hour units. (#1643)
- Use consistent title capitalization for the server activity menu and dialog. (#1659)
- Column comments in the Structure editor now follow each database’s supported behavior. (#1690)
- Closing SQL editor tabs uses less temporary memory. (#1693)
- SQL editor input checks size and line limits without duplicating the full script when validating a dropped file. (#1691)
- Structure views explain when a driver cannot read index or foreign-key metadata. (#1695)
- Searching loaded rows uses less memory on very large values and stops sooner on JSON cells. (#1788)
- SQL completion stays responsive with very large schemas and large scripts. (#1830)
- A MongoDB field that a document lacks now shows as "(missing)" in the grid and "Absent from this document" in the row inspector, instead of an "undecodable" marker. (#1970)
- Polished in-app data grid styling and streaming view pagination states. (#1981)
- The editor bottom bar groups Results and Errors in one switcher, the filter panel and editor toolbar take one row each, and a streamed result no longer blanks while loading or jumps to a Status page after a timeout. (#1873)
- The saved-queries list now opens with Ctrl+Shift+B, because GTK reserves Ctrl+Shift+D for its inspector. (#1957)
- The empty connections page now offers Import connections and Import from URL next to Add Connection. (#1958)
- Long text and JSON cells show a bounded preview in the result grid while Copy Value and View Value retain access to the full value. (#2158)
- Browsing a PostgreSQL, MySQL, SQLite or SQL Server table no longer downloads text cells larger than 1 MiB just to show a preview; View Value fetches the full text on demand. (#1955)
- An SSH host-key or password prompt left unanswered for five minutes now ends the connection attempt. (#2073)
- Streamed query results update with less repeated copying. (#2087)
- Evicting older streamed result pages does less repeated work. (#2157)

### Fixed

- Large cell previews remain compact while copy and export keep the complete value. (#1625)
- Refuse local database file paths that contain non-UTF-8 bytes instead of saving a lossy path. (#1629, #1663)
- Cancelling a paged export is reported as cancellation instead of a source failure. (#1637)
- Software centers classify the published 0.2.0 release as stable and show its release date. (#1642)
- The read-only structure SQL preview no longer shows a blinking insertion caret. (#1644)
- The connection port field now shows five-digit port numbers with room to spare. (#1645)
- Large result counts now use consistent grouping in filtered results and the browse pager. (#1647)
- Skipped statement tabs now show a stopped-state icon. (#1655)
- Gruvbox warning text now uses a distinct, readable color. (#1657)
- Saving a new query to a file now preserves the visible SQL. (#1664)
- PostgreSQL metadata follows implicit schema visibility when resolving a table name. (#1672)
- PostgreSQL column completion now follows each editor session's search path and keeps editor suggestions isolated. (#1673)
- SQL files now receive syntax highlighting in AppImage builds without host GtkSourceView data. (#1677)
- The editor clears its transient closing message after an idle session closes. (#1679)
- Importing a growing file stops at the size limit, and oversized clipboard pastes are refused before parsing. (#1689)
- Invalid agent-token guesses are throttled without locking out valid tokens issued by another process. (#1689)
- Dismissing export setup releases retained result data. (#1699)
- Closing SQL editors releases their text buffers and input contexts. (#1694)
- Tables changed in an editor transaction appear in the sidebar after committing the session. (#1701)
- Dismissing index and foreign-key setup releases the closed forms. (#1702)
- Connection setup shows consistent recovery guidance for connection and metadata errors. (#1758)
- Closing import setup releases its source data and callbacks. (#1704)
- Closing cell edit popovers releases their widgets and callbacks. (#1752)
- Preferences, the quick switcher, saved queries, the full value editor and the CSV import dialog are now freed when closed instead of leaking memory. (#1802)
- Deleted connection drafts stay removed when a late workspace save arrives. (#1798)
- Reimporting a deleted connection restores its saved workspace and rejects stale saves. (#1798)
- Unreadable SQL drafts remain recoverable when connections are deleted or another database workspace is saved. (#1757)
- Closing SQL editors removes their saved draft files after the workspace is saved. (#1759)
- Closing a grid editor releases its cell and popover callbacks instead of retaining them for the session. (#1763)
- Closing Structure tabs releases their views and SQL previews. (#1816)
- The value viewer preserves JSON-looking text without dropping duplicate keys or reformatting numbers. (#1816)
- Refreshing or closing query history releases its rows and menus. (#1819)
- Closing connection organisation releases the colour chooser popover. (#1819)
- Database listing now honors request timeouts and cancellation when switching databases. (#1764)
- Redis command input preserves quoted escapes and binary bytes. (#1780)
- A SQL Server result that exceeds the result size limit now keeps only the first rows, instead of skipping a large row and keeping later ones after it. (#1766)
- Running a script that creates a SQLite trigger together with other statements no longer fails with a transaction-control error on a shared connection. (#1779)
- MySQL stored procedures and text-protocol statements retain their result rows and result sets. (#1773)
- Text using binary collations is displayed as text, while binary data remains bytes. (#1765)
- SQLite columns declared BOOLEAN show a stored integer other than 0 or 1 as that number instead of turning it into true. (#1770)
- The case-insensitive text filter now ignores case on DuckDB and ClickHouse tables instead of matching case-sensitively. (#1771)
- CSV import reads only integer column types as whole numbers, so MySQL spatial and ClickHouse interval columns are imported as exact text instead of failing as non-integers. (#1772)
- SQLite temporal values retain their stored offsets and storage classes. (#1781)
- CSV import auto-mapping prefers exact column-name case before a case-insensitive match. (#1783)
- Stopping a SQLite query now works even when the stop arrives just as the query starts. (#1852)
- A connection's connect and query timeouts can be cleared again after being set, and importing a connection bundle no longer resets them on a connection that already exists. (#1774)
- ClickHouse results preserve non-UTF-8 string bytes. (#1778)
- The SET and ENUM picker popover is now as wide as its widest label, so labels and the Apply button are no longer cut off. (#1968)
- An unreadable saved SQL draft no longer prevents other connections from restoring their workspaces. (#1826)
- Deleted connections no longer reappear in the workspace cache when an earlier restore finishes late. (#1827)
- Unreadable or invalid policy configuration blocks database access instead of silently using default restrictions. (#1832)
- A failed workspace save preserves previously saved editor drafts. (#1829)
- Opening large text values no longer stalls the window. (#1983)
- Read-only SHOW statements such as SHOW STATUS and SHOW VARIABLES no longer ask for write approval on a local connection. (#1959)
- Query history reports export failures instead of silently leaving the file unsaved. (#1760)
- Oversized SQL lines are rejected before the full file is loaded. (#1857)
- Changing a MySQL generated column in the Structure tab keeps its expression instead of failing or dropping it. (#1933)
- MySQL accounts that have no password can now be used to connect. (#1885)
- Approval dialogs no longer display an empty table list. (#1929)
- Refusing SET autocommit=0, SET IMPLICIT_TRANSACTIONS ON or XA START on a shared connection now names that statement instead of showing the BEGIN, COMMIT and ROLLBACK message. (#1954)
- Long saved connection names wrap in narrow connection lists. (#1868)
- A failed workspace save no longer lets restore replace newer unsaved SQL with an older saved draft. (#1964)
- Workspace saves reject invalid destinations before staging SQL and preserve private draft permissions and previously missing saved bodies. (#1965)
- PostgreSQL custom enum values preserve canonical declaration and alter sort order and populate query dialect metadata. (#1987)
- Paging deep into a SQLite table whose composite key contains NULLs no longer skips rows. (#1894)
- A MySQL write preview from an agent is refused when the statement touches a table that cannot be rolled back, instead of reporting a rollback that did not happen. (#1914)
- The row inspector shows text that looks like markup, such as the marker for an undecodable value, instead of a blank field. (#1882)
- A busy PostgreSQL or MySQL connection is no longer treated as lost while long queries are running, so it is not reconnected underneath them. (#1912)
- Timeout retry actions use clear wording for any duration. (#1926)
- Changing a MySQL invisible column in the Structure tab no longer makes it visible. (#1930)
- Exporting or copying a SQLite row whose text contains a NUL byte no longer truncates the generated SQL. (#1892)
- A ClickHouse account with the read-only profile can now connect instead of being refused. (#1901)
- A Redis host that would change the connection address is now refused, and IPv6 hosts connect. (#2025)
- The ER diagram zoom percentage now matches the real zoom. (#2038)
- Editing a MariaDB text cell to or from an empty value no longer stores NULL or fails when EMPTY_STRING_IS_NULL is on. (#1889)
- A ClickHouse query with its own FORMAT clause that produces one enormous line now stops with an error instead of exhausting memory. (#1890)
- Backup, restore and import progress dialogs identify the running operation. (#1934)
- The Structure tab no longer shows a disabled column Comment row for SQLite and DuckDB. (#1956)
- The sidebar can refresh tables and views changed by another client. (#1940)
- Saved-connection and history-filter popovers stay within the application window. (#1931)
- Explain on MySQL 8.4 now shows the query plan instead of an empty window. (#2056)
- The cell editor's focus ring no longer overlaps the text being edited. (#2059)
- Swapping two column names in the Structure tab, or renaming columns in a chain, now saves instead of failing on the server. (#1891)
- Filtering a MySQL FLOAT or SQL Server real column by a value now finds the matching rows. (#1897)
- Saving a grid edit on a SQL Server text column now detects a concurrent change that only differs in letter case or accents. (#1900)
- SQLite, MySQL and MariaDB views appear in the sidebar Views group. (#1944)
- A MongoDB query with stray text between chained calls is now rejected, and a bare JSON filter no longer silently searches an arbitrary collection. (#1893)
- Browsing a Redis database no longer lists the same key twice. (#1895)
- Text filters on MySQL servers that use NO_BACKSLASH_ESCAPES now match percent signs and underscores literally. (#1896)
- A saved connect timeout longer than five seconds now also applies when MySQL or PostgreSQL has to open another connection later. (#1898)
- When an SSH agent holds more keys than the server allows attempts, the error now explains the limit instead of a low-level agent message. (#1904)
- Editor completion closes when another control or dialog takes focus. (#1935)
- Large Redis string previews are identified as incomplete rather than copied or exported as whole values. (#1903)
- A SQLite connection no longer shows an irrelevant connect timeout, and the SQL preview no longer shows a blinking caret. (#1915)
- Version and help flags print information without starting the application. (#1939)
- Direct PostgreSQL and MySQL connections now use TCP keepalive, so a connection whose server silently disappeared is detected instead of hanging. (#1923)
- Tab context menu actions now work. (#1951)
- Sidebar groups show their expanded state while search reveals matching tables and remain usable for blank searches. (#1941)
- Creating a connection no longer overwrites a saved profile when several profiles match. (#2008)
- ER diagrams read tables from their own schemas when names are repeated across attached databases. (#2005)
- Paged results release hidden grids after navigation while keeping cached rows. (#2078)
- Opening a large JSON value no longer serializes its entire contents before showing the preview. (#2030)
- Exporting and importing connections now keeps a SQLite connection's attached database files. (#2032)
- Redis tables no longer offer a filter that cannot work. (#2039)
- Oversized PostgreSQL vector values no longer build unbounded formatted previews. (#2040)
- SQL Server batches report failed transaction cleanup and prevent reuse of connections with uncertain outcomes. (#2042)
- Editing a cell and then deleting its row no longer makes the save fail. (#2060)
- Column types and defaults in the Structure tab can no longer add a second clause to the generated statement. (#2061)
- Row-locking reads (FOR UPDATE, FOR SHARE) are now refused on read-only connections and for agents that may not write. (#2062)
- A raw filter ending in a comment no longer breaks the query, and CSV import into a table whose name contains the word VALUES works on PostgreSQL. (#2063)
- Agents can no longer run more ClickHouse table functions that reach files, other servers or programs, or copy PostgreSQL replication slots. (#2064)
- An agent write preview no longer runs when the connection list cannot be read. (#2067)
- Importing a connection bundle can no longer remove or replace the certificate authority pinned on an existing connection. (#2065)
- Approval prompts and the audit log now name every table an UPDATE or DELETE joins. (#2066)
- ClickHouse, Redis and SQL Server connections now use TCP keepalive, so a connection whose server silently disappeared is detected instead of hanging. (#1924)
- Grid saves detect concurrent text and binary changes even when the column comparison considers the values equal. (#1961)
- Large spreadsheets are rejected before their imported data can exhaust memory. (#2013)
- ER diagrams now report connection and policy failures instead of showing incomplete results as unreadable tables. (#2021)
- An SSH tunnel no longer stops accepting connections after a brief file-descriptor shortage. (#2071)
- Malformed and unknown MCP tool calls now count against the per-token rate limit. (#2074)
- Connecting to an already-missing SQLite or DuckDB database path reports an error instead of creating an empty file. (#2010)
- SQL editor lines no longer clip a few pixels off the top of the text. (#2179)
- PostgreSQL now suggests enabling PgBouncer prepared-statement support when transaction pooling returns this binary-bind error. (#2192)
- Failed workspace saves no longer trigger a duplicate background retry. (#2195)
- Back Up Database keeps a single .dump suffix when the chosen file name already ends in .dump. (#2180)
- Large CSV imports keep the app responsive while their file and insert plan are prepared. (#2171)
- Explain Text tab shows column names for MySQL and other multi-column plans. (#2182)

### Security

- Updated the SSH library so a malicious or compromised SSH server can no longer exhaust BookiE's memory with repeated protocol messages or stall a tunnel by announcing a zero packet size. (#1777)
- Exporting query history as SQL keeps connection names and error text inside comments, so a name containing a line break can no longer add a statement to the exported script. (#1768)
- More malformed MySQL or MariaDB server packets are rejected safely. (#1871)
- After an import fails in a way that may have committed some rows, the audit log records the outcome as unknown and further writes on that connection are held until it is resolved. (#1884)
- Results returned to agents now hide sensitive keys such as passwords inside JSON and document values, not only in sensitive column names. (#1887)
- A policy file that sets two overrides for the same connection is now rejected instead of applying one of them at random. (#1879)
- Database backups are now readable only by their owner. (#2023)
- Agents can no longer read server files or other servers through ClickHouse and SQL Server table functions. (#2024)
- An agent's browse_table request can no longer force Redis to scan its whole keyspace with a huge offset. (#1878)
- A query that was streaming when the app crashed is now recorded as unknown in the audit log on the next start. (#1881)
- Changing the type or nullability of a SQL Server column in the Structure tab no longer removes its dynamic data mask. (#1899)
- A statement run in a SQL session is now checked against the row-count approval threshold using that session's own view of the data. (#1911)
- Writes inside an interactive transaction are now checked against the row-count approval threshold using that transaction's own view of the data. (#1913)

## [0.2.0] - 2026-10-23

Source version **0.2.0** (`Cargo.toml`, inherited by every workspace crate). This section is the user-facing map for the 0.2.0 release since the last tagged package, [0.1.5](#015---2026-09-27). Compare: [`linux-v0.1.5...linux-v0.2.0`](https://github.com/cozyGarage/BookiE/compare/linux-v0.1.5...linux-v0.2.0).

### Added

- Import accepts Excel .xlsx files: the first worksheet with data is read with size, row and column limits and then mapped like a CSV file.
- CSV export has Keep plain numbers as they are: text such as -5 or +1.5 is written without the spreadsheet apostrophe so it re-imports into numeric columns, while other formula-like text is still prefixed.
- Copy as in the grid menu also offers UPDATE Statement and DELETE Statement for a row, found by its primary key. Tables without a key refuse with a message.
- CSV import can set the quote character, backslash escaping of quotes and the file encoding (UTF-8, Windows-1252, UTF-16); a file that is not UTF-8 opens with a matching encoding selected.
- The sidebar table menu has Truncate Table, which asks for confirmation and runs through the same guard as other statements. SQLite is refused because it has no TRUNCATE.
- The sidebar table menu has Rename Table. It asks for the new name and is refused while the table has open tabs, so no pending changes are lost.
- MongoDB connections have an optional Authentication database field, for a user defined in `admin` while the Database field names the database to browse; saved connections and bundles carry it.
- PostgreSQL query cancellation through PgBouncer without interrupting another client's work.
- Unsorted keyed table browsing can move backward through deep pages and jump to the last page using primary-key seeks while keeping rows in ascending key order.
- Arch packages include a build-input manifest with source and dependency archive checksums, package versions, and toolchain details.
- The Arch package includes generated license texts and versioned attributions for the Rust dependencies used by its application binaries.
- Preferences has **Reuse one tab when opening tables**. Turn it off to give every table opened from the sidebar its own tab; an already-open table is still selected instead of duplicated.

- Reveal Table in Sidebar (Ctrl+Shift+L, or the main menu) opens the collapsed groups around the open table, clears a search that hides it, and scrolls to and selects it in the sidebar.
- Opening a new SQL editor while disconnected now shows the editor and removes completion suggestions cached from the previous connection.
- When a query stops at the query timeout, the editor offers Run again with a longer timeout, which runs the same query once with four times the timeout and leaves the saved timeout unchanged.
- A connection marked Production now shows a red Production badge and a tinted header bar, and its window title starts with [Production], so it stands out from other connections.
- SQL completion inserts complete dialect-quoted table and column names, preserves aliases, and offers scoped metadata instead of arbitrary editor words.
- Large SQL query results stream in bounded pages, can be pinned, and can be fully exported by running the guarded query again.
- CSV imports into PostgreSQL, MySQL/MariaDB and SQLite can preserve explicit auto-increment values from mapped columns; generated columns remain database-owned.
- Editable text and JSON columns open a bounded full-value dialog; JSON input is validated, source-text JSON retains its exact text, and Format JSON makes semantic formatting changes visible before Apply.
- At narrow window widths, the navigation sidebar collapses and the Fold block and Unfold all toolbar buttons hide so the remaining editor controls stay in view; their keyboard shortcuts still work.
- Preferences gives appearance its own page and explains the data-browser and editor settings more clearly; About separates project credits from third-party attribution.
- Switching database drivers keeps custom connection values and updates fields that still use the previous driver's defaults.
- Duplicating a password-authenticated connection with a saved password opens the copy's credential form so its password can be entered before connecting.
- PostgreSQL results and edits support `json[]`, `jsonb[]`, `int4range`, `int8range` and `point` values.
- SQL editors can collapse multiline SQL blocks at the cursor and unfold them from the gutter or toolbar.
- Pressing Enter in a connection text field now activates Connect.
- SQL query and pinned result grids can filter loaded rows by a cell's native value without running the SQL again; the grid shows the visible and loaded counts.
- View Value shows pending grid edits, including long text and NULL, before they are saved.
- Find in Loaded Rows searches full loaded text and JSON values beyond the truncated grid preview.
- SQL result filters and sorting stay in place when the same statement is rerun or its result is pinned or unpinned, and selected rows stay attached to the same materialized result.
- Plain-clicking a table opens a temporary preview tab that the next table click can replace; preview tabs are not restored after restart, while editing or choosing **Keep Tab** makes one persistent.
- PostgreSQL and SQLite query plans open in a Tree tab of the Explain dialog.
- Ctrl+V pastes tab-separated text from a spreadsheet into grid cells, all or nothing.
- When a cell belongs to multiple foreign keys, Go to Referenced Row lets you choose the target table.
- ENUM and SET cells on MySQL and MariaDB, and enum cells on PostgreSQL, are edited from a list of members.
- The keyboard selection in an ENUM picker starts on the cell's current member.
- A preference chooses how NULL is displayed in the grid.
- Browse filters can be saved, applied, renamed and deleted per connection, schema and table.
- MySQL and MariaDB connections can use a local Unix socket file, saved with the connection and exported in version 3 connection bundles.
- Query history now opens in a docked panel beneath the workspace, which stays visible while you search and reopen queries.
- Connection subtitles use readable database engine names and avoid repeating the engine when the server version already includes it.
- SQL result grids can sort supported, homogeneous typed columns in memory without re-running the SQL statement; SQL NULL values stay last in either direction.
- SQL editors can show a second side-by-side view of the same query, with independent scrolling and shared edits.
- PostgreSQL schema groups can now be hidden per connection and restored from the schema filter menu.
- PostgreSQL TLS verification now trusts the operating system certificate store when no custom authority is configured.
- MongoDB table browsing now seeks deep pages in `_id` sort order, including collections with mixed key types.
- MongoDB table browsing applies supported column sorts on the server and keeps the requested order across pages.
- MongoDB offset and `_id` keyset pages apply equality, inequality, comparison, range, set-membership, null, `LIKE` and `ILIKE` filters with SQL null behavior.
- Concurrent MongoDB first-page requests now share one schema census and retain the union of types observed across pages.
- Saved SSH hops now have stable IDs and credential revisions, with separate Secret Service operations for per-hop passwords and key passphrases. Legacy connections-file v1 data loads and migrates to v2 on save. Connections use those per-hop secrets for transport; editing a jump chain in the form remains disabled until that editor lands.
- Encrypted connection bundles now write format version 3 with local MySQL socket paths and each versioned SSH hop's password or key passphrase; version 1 and 2 imports remain supported and plaintext bundles still contain no credentials.
- MongoDB browse long text, binary and structured values use guarded, ObjectId-safe `_id` refetches for full values.
- Redis string values now use guarded, byte-safe key refetches for full values; collection and container debug strings remain untruncated.
- SQL-backed table browse cells over 8 KiB now show a typed preview with the original byte count; View Value refetches through the guarded connection by primary key.
- Arbitrary SQL result grids now preview long text, JSON and binary cells with byte counts; View Value reads the complete value from the guarded result already in memory.
- Previewed grid cells no longer keep a second full value in each cell widget.
- Materialized result-grid rows share their source values, reducing duplicate memory for long cells while preserving pending edits on replacement.
- Overlapping View Value requests discard stale replies so an older fetch cannot replace the latest requested cell.
- Long ClickHouse cell values can be opened in View Value up to 12,000 bytes.
- DuckDB BLOB values support guarded exact refetch of the full value from the database.
- MySQL and MariaDB CHAR results preserve `PAD_CHAR_TO_FULL_LENGTH` padding, including VARCHAR, empty values and NULL.
- MySQL and MariaDB CSV imports preserve impossible calendar dates accepted by `ALLOW_INVALID_DATES`.
- PostgreSQL CSV imports preserve native BC date, timestamp and timestamptz values.
- CSV imports into existing tables can remember and restore column mappings for a specific connection and table.
- The sidebar groups tables and views under schemas and groups with object counts; a click collapses a group, a search expands the groups that match, and the choice is remembered per connection.
- A Pin results button in the SQL editor keeps the current results above the results of the next run, and unpins them again.
- Each statement of a script gets a gutter mark after a run: a tick for a statement that returned, a cross for one that failed.
- MySQL ENUM and SET grid edits refuse malformed metadata and preserve escaped labels.
- A failed PostgreSQL statement in the SQL editor shows the line and column of the error.
- A failed statement in the SQL editor offers a Copy error button.
- Selecting several rows and opening the selection badge shows each numeric column's count, sum, average, min and max, with NULLs skipped and a Copy summary button.
- A connection's colour tag now also paints a strip above its workspace tabs, so a window shows which connection it belongs to.
- The connection colour picker shows translated, title-case colour names.
- PostgreSQL shows unsupported built-in ranges explicitly; supported empty ranges stay distinct from SQL NULL.
- MongoDB and Redis browse pages use native fetches when available; MongoDB cursor values remain visible until refresh, and approved edits preserve sibling fields.
- Result column headers show the column type and mark primary-key and foreign-key columns.
- Redis key browsing preserves arbitrary-byte keys and non-UTF-8 string values, and fills requested pages across partial SCAN batches.
- MongoDB filters distinguish missing fields, explicit BSON null, and ordinary values.
- SQL Server query batches now preserve ordered result sets through the policy guard, editor, MCP query responses, and JSON export; MCP shares its row cap across sets, and CSV export refuses multi-set results rather than dropping data.
- ClickHouse Enum8/Enum16 values, arrays and maps preserve declared labels, empty strings and SQL NULL through supported reads and exports; writes refuse undeclared or lossy values.
- PostgreSQL custom enum arrays preserve labels, NULLs, dimensions and bounds through results, parameters and typed CSV import.
- Copy-as-INSERT and SQL exports now let PostgreSQL and SQL Server generate fresh identity values; MySQL continues to preserve explicit auto-increment values.
- Table tabs have a Row inspector pane that shows every column of the selected row with its type and full value.
- The SQL editor shades the statement under the cursor so you can see what Run Statement at Cursor will send.
- The SQL editor jumps to the next or previous statement with Alt+Shift+Down and Alt+Shift+Up.
- Preferences has an Appearance setting to follow the system or always use the light or dark style.
- Result tabs of a script are named after the statement and table, such as 2 · SELECT users (12), instead of Result 2.
- A script with more than five statements shows its results in a drop-down instead of a row of buttons that overflows the window.
- Preferences has an opt-in Vim keys setting for the SQL editor, applied to editors opened afterwards.
- A table remembers the column order you drag it to, per connection.
- Find in Loaded Rows (Ctrl+Alt+F or the grid menu) lists the loaded rows that contain a text and selects the one you pick.
- Holding Ctrl and pressing Tab cycles through recently used tabs; releasing Ctrl opens the selected tab.
- Preferences has an Editor font family setting; an empty value keeps the system monospace font.
- PostgreSQL `bit[]`, `varbit[]`, `citext[]`, `pg_lsn[]`, `macaddr[]`, `macaddr8[]`, `inet[]` and `cidr[]` preserve values and SQL NULL through typed results, bindings, keyed edits and CSV import.
- The result grid's cell menu also opens with Shift+F10, and the editor find fields have accessible names.
- Sidebar table search and saved-filter naming fields have accessible names, and connection colours can be chosen with an accessible button and palette.
- Result grids have a Columns… menu entry to hide and show columns; the choice is remembered per table and at least one column always stays visible.
- Table browse refreshes omit hidden non-key columns while retaining keys; full-row copy and current-page export refuse incomplete rows.
- PostgreSQL `xml[]` preserves values through query results, keyed edits and supported file exports.
- PostgreSQL enum and domain values keep their schema and declared type when names are shadowed or the role has restricted privileges; enum CSV imports distinguish blank cells from NULL.
- PostgreSQL composite arrays, `money[]` and `point[]` remain explicitly unsupported and cannot be written lossily.
- Result grids have View Value…, which opens the whole cell in a read-only window: pretty-printed JSON, a hex dump for binary data, and a Copy value button.
- A saved connection that is still connecting can be cancelled from the progress notice.
- Starting BookiE while it is already running now brings the existing window forward instead of exiting silently.
- The SQL editor has find and replace: Ctrl+F opens a search bar with next, previous, a match count, Replace and Replace All.
- The connection form starts on PostgreSQL, offers file pickers for SQLite and DuckDB files and for the TLS certificate authority, and a TLS failure now says to check the TLS setting.
- A failed connection from the saved list now offers Try Again and Edit Connection instead of only Close.
- Saved connections can be edited from the welcome list, including their password, TLS, timeouts and single-hop SSH settings.
- PostgreSQL enum results, filters, edits and CSV workflows preserve literal `NULL`, empty labels and SQL NULL. UUID-domain and array-domain values retain their declared types, and wide `NUMERIC` grid edits remain exact.
- SQLite STRICT `ANY` values retain their storage classes and SQL NULL through function results and typed CSV restore; JSON aggregate output remains exact in JSON, CSV and XLSX exports.
- DuckDB `TIME_NS` and `TIMESTAMP_NS` preserve all nine fractional digits on parameterized inserts and updates.
- The editor find bar has Match case and Regular expression toggles.
- A Switch database button in the window header lists the databases the account can open on PostgreSQL, MySQL, SQL Server, ClickHouse and MongoDB, reconnects to the one you pick and remembers it for that saved connection.
- The Connect button in the connection form becomes Cancel while it is reaching the server, so a slow or unreachable host no longer has to be waited out.
- The MCP tool list now carries read-only and destructive hints, so clients can tell the metadata tools from the ones that run SQL.
- PostgreSQL databases can be backed up to a file and restored from one with the installed client tools, through SSH tunnels and TLS; restores need approval and both are audited.

### Changed

- SQL completion creates suggestion objects as they are requested, reducing initial allocations for large catalogs.
- The translation template is now named `bookie.pot` and is generated rather than committed, so it no longer conflicts between changes; each release attaches it.
- Opening a SQL editor tab moves keyboard focus into the query instead of leaving typing in the sidebar search.
- Closing or replacing table and query results releases associated data instead of accumulating memory.

- Manually resized and restored grid columns keep their width when the window grows; unsized columns continue to fill the available space.
- Imported connection URLs preserve supported TLS settings and reject options they cannot preserve.
- Ending an aborted PostgreSQL editor transaction reports that it rolled back instead of saying its changes were committed.
- PostgreSQL session audit records distinguish committed transactions recovered through savepoints from aborted transactions that roll back.

- Running a query again keeps the previous result on screen, dimmed, until the new result, error, cancel or timeout page replaces it, instead of leaving a blank area.- An unresolved write outcome blocks governed writes only on its saved connection; a journal failure or unresolved process-wide operation still blocks writes everywhere.
- Typing in a table cell is included in unsaved changes before a tab closes, a connection disconnects or a preview tab is replaced.
- Refreshing a table waits until its pending save finishes, so a failed save does not lose edits.
- An unresolved write outcome blocks governed writes only on its saved connection; a journal failure or unresolved process-wide operation still blocks writes everywhere.
- A write refused because another write is still in flight no longer disables later writes on that connection; unresolved outcomes remain fail-closed.
- MySQL sessions allow locking reads, user-variable assignments, savepoints and supported SHOW forms inside an open transaction; rollback does not undo session effects such as user variables or named locks.
- The MCP token dialog reports whether the one-time token was saved in the system keyring and explains what to do if it was not.
- The Keyboard Shortcuts window splits table browsing into shorter groups, describes mouse selection gestures separately and combines repeated context-sensitive save, undo and cancel actions.
- About, Arch package, Flatpak, and translation metadata link to the BookiE repository.
- Full-table export is refused before a destination file is chosen when a verified read snapshot is not available; current-page and loaded query-result exports remain available.
- MCP data results use the versioned `bookie-tagged-v1` format so clients can preserve exact numeric values.
- Redis in this development line stays a single host/port endpoint; Sentinel and Cluster topologies stay deferred.
- A successful rollback request now warns that session state and other non-transactional side effects may remain after a failed save.
- Startup opens the query history database while the rest of the application starts instead of waiting for it first.
- The Debian package installs its programs without debug symbols, which makes the installed files about a quarter smaller.
- The time shown for a statement no longer includes the wait for your approval on a guarded connection.
- MongoDB browse pages now use a bounded schema sample and stable `_id` paging instead of scanning the full collection for every page.
- Create table from CSV now explains that ClickHouse, MongoDB and Redis connections cannot do it, and DuckDB gets its own JSON type name.
- The Keyboard Shortcuts window groups actions clearly and uses the standard GTK layout.
- Large results use about 45% less memory in the grid because rows are built only when they scroll into view.
- Browse results share their in-memory query result; GTK row objects are created when the view requests them. This avoids eager per-row GObject creation but does not page database rows or bound result memory.
- Grid context-menu CSV copies encode SQL NULL with a collision-free marker, keeping it distinct from empty text and enum labels; formula sanitization remains enabled.
- CSV file and MCP exports show the null marker used for SQL NULL cells and choose one that does not collide with exported text values, so the matching import preserves empty text and marker-shaped labels. Turn off spreadsheet-safe text to round-trip formula-shaped values literally.
- Markdown table exports and copies quote text cells and escape table/inline markup, keeping text labels such as `NULL` distinct from SQL NULL.

### Fixed

- Large SQL files skip automatic editor analysis above 200,000 UTF-8 bytes while preserving their full text for editing, saving and execution.
- Streamed queries build a retained page’s grid only when it is first displayed, avoiding work for hidden pages.

- Large SQL results use Previous/Next controls with row ranges and state when earlier rows are no longer retained.

- Failed MCP token guesses can no longer lock out a valid token: the shared failed-attempt limit applies only after a token fails verification.
- ClickHouse connections that name no certificate authority now trust the system certificate store, so corporate authorities installed on the machine verify; the bundled roots remain the fallback.
- A MySQL CLONE INSTANCE statement with IDENTIFIED BY is no longer saved in query history.
- A named pipe or device chosen as a TLS CA, certificate or key file is refused instead of hanging the connection.
- A TLS certificate failure no longer tells you to set TLS to Disabled; only a server that does not support TLS gets that suggestion.
- PostgreSQL pooled connections are reset (`DISCARD ALL`) when returned, so a `SET`, temporary table or prepared statement from one editor statement no longer leaks into later reads on the same connection. Session mode keeps its own state.
- Stopping or timing out a DuckDB query now interrupts it on the engine, so the connection is free again at once instead of waiting for the query to finish.
- ClickHouse grid edits and deletes check that the key matches exactly one row and refuse, changing nothing, when the non-unique sorting key matches several.
- SQLite refuses to drop a table that other tables reference and names them, instead of silently running their ON DELETE actions on the child rows.
- Pasting rows copied from a BookiE grid restores values that start with `-`, `+`, `=` or `@` and turns the copied NULL marker back into an empty (NULL) cell.
- Restoring a workspace with more than 32 tabs on one connection keeps every SQL editor and drops table tabs first.
- Clearing a cell in a required text column saves an empty string instead of refusing the edit with "Field is required".
- The connection dialog puts Test on the left and Connect on the right with a centered title; its dropdown choices, port and timeout boxes have an arrow or room around them.
- A long message in the SQL editor toolbar, such as the notice after cancelling a large query, is cut with an ellipsis instead of pushing Pin results, Session, Explain, Run and Split off screen; the full text is in the tooltip and on the Status page.
- A `USE` run on a pooled MySQL connection no longer changes which database later catalog reads and queries see: each pooled connection returns to the configured database before it is reused.
- Keyboard run shortcuts no longer start another execution while a query is active.
- After a tunnel drop during a Session transaction, a fresh editor session can run read queries again: ending its read-only snapshot with ROLLBACK is no longer refused by the earlier unresolved write, while commits and writes on that connection stay blocked.
- Cancelling a PostgreSQL backup removes its partial file; a started restore that is cancelled or fails leaves governed writes on that connection blocked after reconnect while its outcome remains unknown.
- Read-only connections refuse grid edits through alternate actions while still allowing local drafts to be discarded or undone and redone without changing stored data.
- Structure column expanders now point right when collapsed and down when open.
- A database switch rejected during another connection change no longer alters the saved connection.
- On MySQL and MariaDB, `--` followed by a character other than a space, such as `SELECT 5--2`, no longer hides the rest of the line as a comment, so the following statements are found and checked separately.
- A save or import whose connection was lost during a statement could be recorded as rolled back, and agents were told the rollback was confirmed; it is now recorded as an unknown outcome, and an import batch that partly applied is no longer recorded as rolled back.
- Closing a retired database session after a tunnel loss no longer sends ROLLBACK through that unusable session; the transaction outcome remains unknown.
- A PostgreSQL identifier containing dollar signs, such as `my$a$b`, no longer makes the editor read the rest of the script as one quoted statement.
- The error location for a PostgreSQL SELECT shown page by page pointed past the end of the statement; it now marks the right place.
- Renaming a MySQL table in a database other than the connection's moved it into the connection's database, and renaming a ClickHouse table failed.
- In a Session editor tab, a PostgreSQL SELECT with a typo or an error partway through no longer ends the session and its temporary tables, and Export all rows of such a result reads through the same session.
- A MySQL `TINYINT(1)` holding a value other than 0 or 1 showed as true, and saving another change in its row could report that the row had changed; it now shows the stored number.
- SQL Server `real`, MySQL `FLOAT` and DuckDB `FLOAT` values showed extra digits, such as 0.10000000149011612 for 0.1, in the grid, copies and exports.
- Streamed query results: a result of exactly a whole number of pages no longer ends on an empty page, a failed or timed-out stream is shown first, a stream still loading can no longer be pinned as if complete, and pinned results stay visible while a query runs or after it is stopped.
- MCP clients with valid tokens were refused with a rate-limit error once all agents together exceeded one token's per-minute limit; only failed sign-in attempts now count toward the shared limit.
- Dropping a SQL Server column that has a default, or changing its type, failed in the Structure tab; the default is now removed first and kept after a type change.
- Importing a connections bundle or deleting a connection while the same connection was being saved could leave its record and its saved passwords from different edits; these now take turns.
- Saving the same connection from two windows at once, or switching its database while an edit was saving, could lose one of the changes or pair one edit's server with the other's password.
- A SQL Server connection with TLS set to Prefer failed against a server that offers no encryption; it now continues without TLS, as Prefer does for PostgreSQL and MySQL, and a server that refuses encryption under Require reports a TLS failure.
- A ClickHouse, Redis or MongoDB save that failed partway now says the changes before the failing one were applied, and the audit journal no longer records it as rolled back.
- Creating a SQLite table with a foreign key in the Structure tab failed; the key is now declared with the table, and Add Foreign Key is unavailable on existing SQLite tables, which cannot take one.
- SQLite table, index and foreign-key metadata now remains available when user tables use built-in PRAGMA function names.
- A read-only query that failed, hit its row limit or lost its connection no longer blocks saving changes until the next reconnect.

- PostgreSQL completion keeps quoted case, dots, spaces and embedded quotes distinct when loading table columns.
- PostgreSQL errors no longer replay queries or writes merely because their error code resembles a stale prepared plan.
- Browse pagination now disables Next at the known end of a table and ignores exhausted-page navigation requests.
- The CSV import progress dialog now closes after the import completes or is cancelled.
- Explain view tabs and the schema filter button now use icons available in Adwaita and Yaru themes.
- The Explain plan window now closes when you press Escape.
- Pinned results no longer shift the statement numbers shown for a new script's results or errors.
- View Value wraps long unbroken values without adding display-only hyphens.
- Editing a table's structure keeps the Structure label on its tab when unsaved changes are marked.
- Open Browse tabs reload their rows after Structure changes so grid headers and SQL exports use the current column names.
- Opening the filter strip with an active structured filter now expands the rule editor instead of showing an empty raw WHERE field.
- Restoring an editor no longer triggers a GTK sidebar layout failure before the window has completed its first layout.
- PostgreSQL `SELECT ... FOR UPDATE` statements are treated as writes and follow configured approval and read-only rules.
- PostgreSQL displays `interval` and `timetz` values and formats `float4` values consistently, including inside point and float arrays.
- PostgreSQL grid filters on real-valued columns compare values using their native type, and stale prepared plans no longer reuse an incompatible result type.
- PostgreSQL temporal column changes preserve the declared precision and retain the existing default expression.
- Narrowing a column type from the Structure tab no longer silently truncates stored values.
- Structure comparisons detect an index or foreign key replaced under the same name, and remove the constraint that owns an index when requested.
- Adding a primary key or identity to a new column preserves the requested property; toggling either on an existing PostgreSQL column is refused when the operation cannot be applied safely.
- PostgreSQL XML, OID, `tsvector` and geometric values now decode or write through their exact text forms where the driver supports them.
- PostgreSQL `json` values retain their original text through result decoding and value carriers.
- PostgreSQL EXPLAIN runs the statement at the cursor rather than the entire editor buffer.
- Undoing an edit that reverted a cell restores the earlier pending edit.
- Draft cells explicitly set to NULL override the database column default.
- Local text sorting follows the system locale, and sorting a truncated result warns that only loaded rows are included.
- Spreadsheet paste preserves leading quotes and round-trips copied cell text, including embedded line breaks.
- A failed cell save no longer allows a second save or a new edit to overwrite its in-flight state.
- A busy connection pool is reported as busy rather than as a disconnected connection.
- Corrupt saved window or column sizes are clamped, and invalid geometry no longer overwrites the last valid layout.
- Exporting query results no longer duplicates the selected file extension when the file chooser adds it automatically.
- Screen readers can activate the connection form's Driver, Endpoint and TLS selectors and identify its port and timeout inputs.
- The docked query history panel rebuilds each time it opens, so its connection filter lists connections added, renamed or deleted since it was last shown.
- PostgreSQL browse pages for tables without a primary key now use the row locator as a stable ordering tie-breaker, preventing duplicates and gaps between static pages.
- Filtering a PostgreSQL column of type `time`, `timetz`, `interval`, `inet`, `bit`, a range, an enum whose name contains a type word such as `update_kind`, or a domain now builds a valid filter instead of misreading the value or comparing it as text.
- CSV import into PostgreSQL reads enum and domain columns, bit strings and ranges as text, so an enum named like `maintenance_status` or a `bit(3)` value no longer fails as "not a whole number".
- Create table from CSV keeps zip codes, phone numbers and other identifiers with a leading zero or plus as text instead of turning `01234` into `1234`.
- Identifier quoting keeps control characters in a name instead of removing them, so a statement can no longer target a different table than the one named.
- PostgreSQL edits that set a cell to NULL, compare an old NULL, leave a column NULL in a new row, or import an empty field into an integer, date, uuid, boolean, numeric, JSON or inet column no longer fail with "expression is of type text".
- After a table changes (for example a column is added from the Structure tab or another client), `SELECT *` on PostgreSQL shows the new column instead of failing with "cached plan must not change result type" on every refresh.
- PostgreSQL grid edits, inserts, key lookups and deletes now work for `inet`, `cidr`, `macaddr`, `interval`, `bit`, `varbit`, padded `char(n)` keys and `json` columns. XML, OID, `tsvector`, `regclass`, `char`, `record`, `hstore` and `ltree` scalar results now use PostgreSQL's text representation; supported types can be bound back through text.
- PostgreSQL `EXPLAIN (ANALYSE)` of a write is classified and gated like the write instead of as a read.
- The `query_to_xml` family, `crosstab`, `pg_notify`, `pg_logical_*`, `pg_replication_*`, `pg_stat_reset*`, `pg_ls_*` and more large-object functions are administrative, so agents are denied and read-only connections refuse them.
- A constant-true predicate (`WHERE true`, `WHERE 1`, `WHERE 1=1`, `WHERE 2 > 1`, `WHERE id = 1 OR true`) counts as unscoped DML. Predicates that depend on a column (`WHERE id = id`) are not evaluated and stay scoped.
- Grid headers and Go to Referenced Row now pick up foreign keys that arrive after the first draw; before, the redraw reused the old headers and menu.
- Go to Referenced Row on a column that belongs to several foreign keys offers the usable ones even when another key has a NULL value.
- Picking a PostgreSQL enum label no longer fails when the enum's type name looks like a built-in type such as `date_format` or `timestamp_source`.
- Opening a second table over a preview tab, dropping a table with open tabs and promoting a new structure tab now remove the old tab page instead of leaving it on screen without its controller, which aborted the app when clicked.
- A query stopped by a timeout now says "Query exceeded the Ns query timeout." instead of claiming the timeout was configured in Preferences, which was wrong for a per-connection timeout.
- Closing an editor tab no longer leaves it on screen stuck on "Rolling back and closing session…", including after choosing Save for a file tab; the stale tab used to abort the app when Split was clicked.
- Screen readers announce the selected-row count on the selection summary.
- Browse grid headers now show the primary key marker, and the foreign key marker and Go to Referenced Row appear even when the foreign keys arrive after the first draw.
- Reopening query history refreshes the list so queries run while it was closed appear immediately.
- PostgreSQL connections accept a bracketed IPv6 host such as `[::1]`.
- A PostgreSQL login the server refuses (wrong password or missing database) is reported at once instead of being retried, and a pooler in transaction mode gets an explanatory hint on prepared-statement errors.
- CSV imports can map explicit auto-increment values on PostgreSQL, MySQL/MariaDB and SQLite while generated columns remain excluded.
- Grid edits and spreadsheet paste preserve line breaks inside text values.

- MySQL and MariaDB connections with TLS Disabled no longer use a saved client certificate or key
- The window opens without waiting for old query history to be pruned at startup
- Undoing or redoing one of several edits to the same cell keeps the earlier edit pending against the value stored in the database, so saving no longer drops it or reports a changed row
- A table's column and foreign key lists from a superseded refresh are no longer applied after a newer refresh started
- Returning to a table after a Structure rename refreshes its browse columns before fetching rows, so exports use the current column names.
- Saving the window layout no longer waits for a slow disk read at startup, so the window stays responsive while saved tabs load
- Pressing Cancel while a connection is finishing no longer opens that connection anyway
- Refreshing, closing or pressing Kill twice in the activity dialog no longer cancels a Kill that is already running, so its result is always shown
- A run that cannot start (the session is closing or the connection is gone) no longer hides the result of the query still running in that editor
- Deleting or pinning several history entries in quick succession no longer brings a deleted entry back into the list
- Refreshing the table list and the editor's column suggestions now cancel the previous request at the database instead of letting it run on after a newer one or a connection change
- Editing a number so that only its stored form changes (`1.5` to `1.50`, `0` to `-0`) is now saved instead of being dropped as no change
- Copy Value preserves full text and JSON values, including previewed, pending and draft values; binary cells currently copy a byte-count label only ([#1587](https://github.com/cozyGarage/BookiE/issues/1587)). Editing literal `<NULL>` text no longer turns it into an empty value.
- MongoDB connections now reject malformed or URI-shaped host input before constructing the connection URI.
- Closing a table tab, paging, or disconnecting now stops the slow read it started on the server instead of letting it run to the timeout.
- PostgreSQL optimistic grid edits cast both new and previously read custom enum, enum-array, and domain values to qualified native types, so stale-row comparisons work across those columns.
- A PostgreSQL query that returns more rows than the result cap now returns the capped rows promptly and stops the server query, instead of waiting for the server to finish or timing out.
- Rebuilding the connection list no longer triggers GTK errors that could abort the app.
- Saving an edited cell or deleting a row in a PostgreSQL, MySQL, SQL Server, SQLite or DuckDB table no longer asks for a manual write approval every time; the write is already limited to one row per statement.
- MongoDB stale grid deletes now compare the document's full top-level field set as well as materialized values, so a field added after the read prevents deleting that changed document.


- Right-clicking an editable result cell opened the cell editor's own menu on top of the grid menu; only the grid menu opens now.
- PostgreSQL enum-array XLSX exports preserve two-dimensional values and non-default lower bounds as text through LibreOffice Calc and Gnumeric re-saves, including empty labels, literal `NULL`, SQL NULL elements and formula-shaped labels.
- MySQL query results keep their projected column metadata when no rows match, including inside transactions, and transaction queries obey the shared result cap.
- MySQL multi-statement saves refuse non-DML statements before dispatch and distinguish rollback failure; successful rollback requests no longer imply that non-transactional table writes were reversed.
- SQL Server rowversion and temporal period columns are read-only and omitted from generated INSERT statements, so copying or inserting rows does not try to overwrite server-owned values.
- CSV imports accept the `\x` hexadecimal blob format written by exports, preserving empty blobs and NULL separately.
- Malformed DuckDB script tails fail closed instead of executing a valid prefix and silently ignoring the remainder.
- DuckDB SQL literals preserve exact `rust_decimal` values with explicit precision and scale casts, including the crate's mantissa and scale limits.
- Formula-safe decimal CSV imports reject excess precision instead of rounding it.
- CSV imports preserve ClickHouse `Int128` and `UInt128` values exactly, including values prefixed by formula-safe apostrophes, instead of rejecting them as i64.
- Editing PostgreSQL array cells now casts their text value to the matching built-in array type instead of failing with a type mismatch.
- MongoDB browse pages scan field types with the same cursor used for page rows and keep conflicting BSON values read-only.
- MySQL pooled connections use UTC for `TIMESTAMP` decoding; dedicated sessions refuse non-UTC `TIMESTAMP` values instead of presenting a wrong instant.
- SQL Server `money` and `smallmoney` results are refused as undecodable instead of exposing the TDS client's rounded float conversion.
- SQL Server legacy `datetime` values decode only when their 1/300-second ticks map exactly to nanoseconds; inexact ticks remain undecodable.
- Cancelling a connection attempt over the system OpenSSH client now stops its ssh master process and cleans up its private directory, instead of leaving it running in the background.
- If the app is killed or crashes while using the system OpenSSH client, its ssh master process now exits with it instead of continuing to run in the background until the next launch.
- The "Saving in progress" dialog shown while closing a window now closes when the save ends, instead of staying open if the save failed.
- The structure editor now refuses column names that differ only in case on databases that ignore case, and refuses duplicate index names, before sending the change to the server.
- The filter bar now offers comparison operators for MySQL `MEDIUMINT` and PostgreSQL `time without time zone` columns instead of text matching.
- SQL completion now finds the statement under the cursor correctly when a comment or a PostgreSQL `$$` body contains a semicolon.
- A database driver that crashes while a write's size is being estimated, or while a session closes, now produces an error and a connection replacement instead of leaving the window waiting.
- MCP `export_data` with CSV now reports `truncated` when the row cap cut the result, as the JSON format already did.
- An MCP write preview that is cancelled or times out while its transaction is starting now says the outcome is unknown, instead of reporting a plain cancellation.
- Query history is left untouched and switched off, instead of being opened as if it were current, when its file was written by a newer version of the app.
- Saving a favorite or editing saved queries while the favorites list is still loading can no longer bring back an older list.
- Duplicating a connection while the keyring cannot be read now offers to enter a password for the copy, instead of assuming the original had none.
- Closing the Explain window now stops its query instead of letting it run until the query timeout.
- SQL Server queries that return several result sets now show every set inside an editor session or transaction, masked and audited like the first, instead of only the first.
- A MySQL or MariaDB connection that carries a PostgreSQL-style socket directory is now refused when it connects, instead of being checked against a PostgreSQL socket file.
- The connection form's Environment and Method choices can now be operated with a screen reader and other assistive tools.
- After a transaction start is interrupted with an unknown outcome (for example an MCP write preview that times out), the connection stops accepting governed writes until it is replaced, instead of carrying on as if nothing had happened.
- PostgreSQL column completion now tells apart a quoted mixed-case table such as "Users" from users, and keeps dots and doubled quotes inside quoted names.
- Browsing or querying a MongoDB collection no longer holds a whole sample of documents in memory while it works out the column types, which could exhaust memory on collections with very large documents.
- Pasting several columns into the grid now follows the order you see, skipping hidden columns, instead of the table's own column order; pasting from a hidden column is refused.
- Browse filters Contains, Starts with and Ends with now match a literal % or _ typed in the search on SQLite, DuckDB and SQL Server, and a typed [ on SQL Server, instead of treating them as wildcards or returning wrong rows.
- The SQLite table list now shows user tables whose names start with sqlite followed by another character, such as sqlite3_results.
- The grid's Set to NULL action now refuses read-only cells and columns that do not accept NULL, with a message, instead of staging a change that would fail on save.
- SQLite databases whose file names contain a question mark or a percent sign now open that exact file instead of a different one.
- Driver panics during connection capability or session health checks no longer escape PolicyGuard; the affected connection is reported unusable for replacement.

### Security

- Update TLS certificate verification and bundled trust roots to block distrusted emSign certificate chains.

- A masked agent whose ClickHouse, Redis or MongoDB batch failed partway saw the database's own error text; it now gets the same withheld message as other failures.
- A Redis `SELECT n` was treated as a read, so read-only connections and agent read access let it switch the shared connection to another database; it now needs the approval an unrecognised statement needs.
- A `FOR UPDATE` or `FOR SHARE` read inside a WHERE or HAVING subquery, a join condition or a function argument is now treated as a locking read, so read-only connections and agent read access refuse it.
- A host key listed as `@revoked` in known_hosts with a wildcard or negated host pattern, such as `@revoked * ssh-ed25519 …`, is now refused instead of being ignored.
- Unknown host keys on configured SSH jump hosts now require explicit confirmation before connecting.
- Statements that set a password, an encryption key or a connection secret, and text that does not end its strings or comments, are no longer saved in the query history.
- An agent's write preview on MySQL no longer runs DDL or other statements that MySQL commits immediately; they are refused inside a transaction instead of being applied and reported as rolled back.
- SQL Server `BEGIN TRAN`, `COMMIT TRAN` and `ROLLBACK TRAN` are now treated as transaction control, so a shared connection refuses them and a session tracks the transaction they open instead of auditing later writes as committed.
- Agent result masking now treats `to_json` / `to_jsonb` and JSON object builders the same as `row_to_json` when deciding which columns to redact.
- Agent result masking now redacts columns whose projection source cannot be resolved, and masks whole-row wildcard arguments instead of leaving them clear.
- A session or governed-write lockdown refusal is now recorded in the audit journal instead of ending without a terminal audit state.
- `SELECT … INTO` is now treated as a write, so read-only connections and agent read access refuse it instead of letting it create a table.
- The row-count estimate that guards large writes no longer runs a side-effecting function in your WHERE clause before you approve the statement; such statements ask for approval without an estimate.
- A session now refuses a script that hides BEGIN, COMMIT or ROLLBACK among other statements, so the transaction BookiE tracks always matches the one on the server.
- A SQLite, PostgreSQL, MySQL or MariaDB connection marked read-only is now read-only in the database engine itself, so a statement the safety checks mistake for a read still cannot change data; a read-only SQLite connection no longer creates a missing file.
- Importing a connection bundle now refuses a file over 16 MiB, or anything that is not a regular file, before reading it.
- Panic diagnostics retain source locations and requested backtraces while always withholding the panic message, including with `TABLEPRO_DEBUG_PANICS=1`.
- SSH connections refuse a host key marked revoked in `known_hosts` and ignore a relative configuration directory.
- Agent token files are created with owner-only permissions, and approval prompts strip bidirectional text controls that could disguise displayed text.
- Shortened agent-token labels stop at character boundaries and never split a UTF-8 character.
- Bulk import approval is bound to the requested target, and read-only sessions refuse `BEGIN READ WRITE`.
- Controlled batch approval interruptions receive a terminal audit outcome.
- Known transaction errors no longer erase prior successful write effects from the audit record.
- Agent masking covers PostgreSQL XML serialization results and redacts driver errors that could contain masked values.
- Administrative PostgreSQL function matching respects qualified names.
- Constant numeric predicates are evaluated exactly when checking whether a write is scoped.
- The built-in SSH client is updated to a release that fixes recent upstream security advisories.

## [0.1.5] - 2026-09-27
- Choosing the system OpenSSH client inside the Flatpak build now stops immediately with a message to use the built-in SSH client.
- The Debian package now includes the SSH password helper that the system OpenSSH client needs.
- A lost connection stops reconnecting when the password, TLS setting or configuration is wrong and says why, instead of retrying forever.
- SQL completion no longer mixes up the columns of tables that share a name in different schemas.
- In the headless agent, a write interrupted on one connection no longer blocks governed writes on every other connection until restart.
- The headless agent now replaces a cached connection after a driver panic or an unusable-connection fault instead of reusing it because its health check still answers.

### Added
- Loaded results can be exported as Markdown, HTML, XML, SQL INSERT statements, or an Excel workbook, alongside CSV and JSON. Statement export is offered only for connections whose engine can express SQL literals, and an Excel export exceeding worksheet limits leaves any existing destination untouched. Workbooks preserve fractional seconds, timezone-bearing timestamps and dates outside 1900–9999 as exact text; ordinary supported dates and whole-second times remain native spreadsheet cells. Empty text is refused with CSV/JSON alternatives instead of being silently collapsed into NULL, preserving any existing destination. XML and HTML preserve carriage returns when parsed. XML refuses unsupported XML 1.0 characters and HTML refuses NUL characters with a row/column error instead of replacing or dropping them.
- Saved queries have a management dialog (Ctrl+Shift+D): search them, open one in a new editor tab, rename it, or delete it. Until now a saved query could only be reached through the quick switcher and could not be removed.
- A query that runs for 10 seconds or longer shows a desktop notification when it finishes while the window is not focused.
- Ctrl+O opens a .sql file in a new editor tab. A file larger than the editor's limit is refused with an explanation instead of being cut short.
- An editor tab opened from a .sql file is titled by the file, marked while it has unsaved changes, and saves back to it with Ctrl+S; Ctrl+Shift+S saves to a new file, and Ctrl+S on an unsaved query asks where to save it. If another program changed or removed the file since it was opened, the save stops and offers Overwrite or Save As instead of replacing the other program's changes. Closing a tab with unsaved file changes asks first, and after a restart the tab is linked to its file again; if the file is gone, the tab keeps its text and says it is no longer linked.
- Query history now also records the statements the app itself runs when you save a table structure change or save edited rows, and the history dialog can filter by where a statement came from.
- Logs can be written as structured JSON lines by setting the log format environment variable, for collection by a log agent. Any other value keeps the human-readable format.
- A CSV file can be loaded into an existing table from the table's context menu. The file's separator and header row are detected and can be changed, each table column is mapped to a field before anything runs, and the whole import is approved once. It commits in bounded batches, shows progress, can be cancelled, and reports how many rows were written when it stops early.
- A CSV file can also create the table it is loaded into, from the schema header in the sidebar. The column names and a type guessed from the first rows of the file are shown for every column and can be changed before the table is created. A guess falls back to text whenever any value in the sample does not fit a narrower type.
- The structure editor shows a Comment field for each column, and reads each column's comment on PostgreSQL, MySQL, SQL Server and ClickHouse, and writes it when creating a table, adding a column or editing one. SQLite refuses a comment with a clear message instead of dropping it silently.
- Open Quickly (Ctrl+P) lists the connection's tables and views across every schema. Typing a name or a schema-qualified name such as `sales.orders` finds them, and choosing one opens it.
- A browsed table's cell menu has Filter by This Value, which narrows the table to rows holding that value, or to rows where the column is NULL for an empty cell. It replaces an existing rule on the same column and keeps the others.
- PostgreSQL sessions opened by the app report the application name BookiE, and those opened for agents report BookiE agent, so they can be told apart in `pg_stat_activity`.
- PostgreSQL materialized views are listed with views in the sidebar and open read-only.
- The built-in SSH client can authenticate with keys held by ssh-agent, chosen as "SSH agent" in the SSH section, including in the Flatpak build; a missing agent or an agent with no keys is reported plainly.
- An SSH tunnel can use the system OpenSSH client instead of the built-in one, so `~/.ssh/config`, ProxyJump, ssh-agent and host certificates apply. Host keys are always checked: the app asks before trusting a new host even if `~/.ssh/config` would accept it, and the agent daemon refuses an unknown host. A saved password or passphrase is only given to the host or key it belongs to, never to a jump host. Not available in the Flatpak build, which cannot run the host's ssh.
- An editor tab can turn on Session to run on its own PostgreSQL, MySQL, SQL Server or SQLite file connection, so settings, temporary tables and a transaction started with BEGIN carry over between runs. The button shows when a transaction is open, and turning Session off or closing the tab with one open asks whether to commit or roll back. Every statement still goes through policy, approval and the audit journal, and COMMIT and ROLLBACK are recorded as transaction outcomes.
- A Catalog window in the main menu lists a PostgreSQL connection's functions and procedures, triggers, sequences, extensions, roles, user-defined types and each table's privileges by role, optionally within one schema. An engine without catalog support, a listing refused by policy, a failed listing and an empty one each say so differently.
- Agents can list a PostgreSQL connection's routines, triggers, sequences, extensions, roles, user-defined types and table privileges through the new `list_objects` MCP tool, optionally within one schema. It needs only a read token, is limited to the token's allowed connections, and is audited like other catalog reads; an engine without catalog support answers that the listing is unsupported rather than returning an empty list.
- A saved connection can have its own connect timeout and query timeout, set when it is created. The connect timeout also applies to agent connections; 0 keeps the defaults.
- A saved connection can be given a name; leaving the name blank keeps the address-derived label.
- A saved connection can be duplicated; the copy starts without saved credentials.
- A saved connection can carry a colour tag from a fixed palette, shown beside it in the connection list.
- The connection list is split into a section per group, with ungrouped connections in their own section.
- Saved connections can be exported to and imported from a connection bundle file. A bundle without a passphrase carries no credentials, a passphrase-protected bundle encrypts them, and an import shows every connection and what will happen to it before anything is written.

### Changed

- The built-in SSH client's unknown-host-key error no longer suggests connecting once with the system `ssh` command, which writes to a different `known_hosts` file than the one this client reads.
- A built-in SSH host-key prompt now waits until its button response is handled before treating a close attempt as a decline, so trusting a host persists the key.
- The built-in SSH client reports plainly when a server accepts only keyboard-interactive authentication, which it does not support, instead of a generic authentication failure.
- The built-in SSH client now sends keepalives and can report a tunnel as closed once a bastion stops responding, instead of leaving a dead tunnel looking alive until a database driver's own timeout notices.
- When a saved password can't be read because no keyring is running, the keyring is locked, or its unlock prompt was cancelled, the connection error now says which one and what to do, instead of showing the raw D-Bus error.
- The keyboard-shortcuts dialog and date editor now use the GNOME 50 APIs.
- Development builds isolate connections, history, audit, drafts, settings, the policy file, locks, and keyring items from installed builds.
- Column-width and filter persistence is owned by the application and shared explicitly with its windows and tabs.
- Query history is owned by the application and passed explicitly to editors, dialogs, preferences, and retention work.
- Workspace cache, coalesced writer, and close-time flush are owned by the application and shared explicitly across windows. A connection whose editor draft cannot be read no longer blocks restoring every other open connection's tabs, and deleting a saved connection now removes its drafts from disk instead of leaving them behind.
- Database connections, user preferences, and the MCP agent bridge are owned by the application and shared explicitly with its windows and dialogs.

### Fixed

- SQL Server `DECIMAL(38,38)` values no longer panic the driver and retain exact text when they exceed the editable decimal range.
- SQL Server `sql_variant` values keep their native column type and show as undecodable instead of being mislabeled as a base scalar.
- Browse shortcuts for Left, Right, and space showed keypad keys, and click shortcuts showed Shift and Ctrl alone.
- New connection opened on the first driver with port 5432, database postgres, and username postgres.
- Browse shortcuts for Left, Right, and space showed a blank key.
- Click shortcuts were labeled Pointer_Button1.
- Test and Connect sat at opposite ends of the connection header, and a long title ran into the buttons.
- A ClickHouse connection with Verify CA or Verify Full through an SSH tunnel checked the certificate against the tunnel's local address instead of the database's own hostname, so both modes always failed. They now verify against the database's hostname.
- A MongoDB connection with Verify CA or Verify Full through an SSH tunnel checked the certificate against the tunnel's local address instead of the database's own hostname, so both modes always failed. They now verify against the database's hostname.
- A Redis connection with Verify CA or Verify Full through an SSH tunnel checked the certificate against the tunnel's local address instead of the database's own hostname, so both modes always failed. They now verify against the database's hostname.
- A session whose connection was lost now refuses any further statement instead of dispatching it and letting the resulting error turn off governed writes for every connection.
- Stopping the MCP agent server while a write was still in progress could tear down the server before that write reached a recorded outcome, leaving its audit entry open. The server now waits at least as long as a write is allowed to run before it finishes shutting down, so an interrupted write is always recorded as cancelled or unknown.
- A session that loses its connection mid-transaction now has its transaction outcome recorded as uncertain, and the session itself stops accepting statements even if the driver still reports it usable.
- A session COMMIT with no preceding BEGIN, and a BEGIN sent while a session transaction is already open, are now refused instead of being sent to the database unaudited.
- A session COMMIT that only succeeded because PostgreSQL had already rolled back an aborted transaction is now audited as rolled back instead of committed.
- A cancelled or timed-out session COMMIT or ROLLBACK is now audited with a cancelled or timed-out outcome instead of a generic failure.
- A session COMMIT that fails because the database already ended the transaction, such as a deferred foreign key violation, no longer leaves the session's transaction marked open.
- MongoDB nested documents, arrays and uncommon top-level BSON kinds retain their BSON markers as canonical Extended JSON instead of debug text. Nested documents and arrays plus top-level BSON Timestamp, regex and MinKey cells can be edited in the grid and write back as native BSON; canonical Extended JSON imports reconstruct nested Decimal128, date and binary subtypes. Real-server JSON, CSV and XLSX exports and the MCP browse tool preserve those markers.
- CSV imports into SQLite INTEGER, REAL and NUMERIC affinity columns pass unparseable values as text, allowing SQLite to store them without rejecting valid mixed storage classes.
- Duplicate Row refuses rows with undecodable cells or a mismatched cell count instead of inserting a silently substituted NULL.
- ClickHouse rejects response rows whose value count does not match the column metadata instead of padding or truncating the row.
- A driver that reports its connection as disconnected now triggers an immediate reconnect attempt instead of waiting up to 30 seconds for the next scheduled connection check.
- SQL Server decimals wider than the editable range keep their sign and digits. A negative value no longer shows a minus sign inside its fraction, and a whole number no longer gains a trailing ".0".
- MySQL zero dates such as 0000-00-00 are shown as their stored text instead of an empty cell. Negative TIME values keep their sign, times outside one day and dates with zero parts are shown exactly instead of as undecodable, and YEAR values appear as numbers. All of them can be written back through edits, parameters and SQL exports.
- DuckDB dates, times, timestamps and enum labels decode as usable values instead of debug strings. Temporal precision, timezone instants, infinities and end-of-day times survive parameter and SQL round trips; unsupported intervals, collections and calendar ranges remain visibly undecodable and cannot be written back.

- PostgreSQL intervals retain independently signed fields under every interval style, and extreme durations use a re-importable seconds representation. Date and timestamp infinities remain distinct from NULL.

- PostgreSQL arrays of common scalar and temporal types preserve NULL elements, empty values, nesting, lower bounds and exact numeric digits through query results and SQL exports. int2vector and oidvector values keep their space-separated form. Unsupported element types and oversized decoded arrays remain visibly undecodable.

- PostgreSQL end-of-day times remain 24:00:00 instead of silently becoming midnight, and times with time zones retain their stored offsets and microseconds through queries and SQL exports.

- PostgreSQL SQL exports preserve BC dates and years above 9999 using valid era and year syntax, including fractional timestamps and UTC instants.

- PostgreSQL numeric results preserve wide integers, long fractions, decimal scale, NaN and infinities. Values beyond the editable decimal range remain exact text instead of appearing undecodable.

- Excel exports preserve integers beyond 15 digits and all exact decimals as text cells, retaining every digit and decimal scale. These cells require explicit numeric conversion for spreadsheet arithmetic.

- Numeric input in grid edits, filters and CSV imports rejects overflow to infinity and nonzero underflow to zero. Named decimal parameters stay exact, SQLite still compares them as numbers, and automatic parameter typing keeps values as text when they exceed the exact decimal range.
- SQL copy and export use scientific float literals to avoid tiny values becoming zero or overflowing an engine's decimal parser; ClickHouse decimal literals retain their full precision.
- Redis commands preserve empty quoted arguments instead of dropping them.
- DuckDB accepts bound parameters in result queries instead of reporting the operation as unimplemented. Decimal parameters and results stay exact numbers, so arithmetic on a bound decimal is no longer rounded to a whole number. DATE and microsecond TIME/TIMESTAMP parameters bind natively. Finer-than-microsecond TIME/TIMESTAMP and all TIMESTAMPTZ parameters remain exact text and may need an explicit cast for expressions.
- MySQL and ClickHouse decimals retain their exact digits, including values beyond the editable decimal range. Grid edits and filters reject decimals that would need rounding.
- ClickHouse non-finite numbers remain distinct from NULL, and JSON exports and agent responses retain NaN and infinity as text instead of substituting NULL.
- SQL Server INSERT exports preserve Unicode text and render booleans as valid numeric literals.
- App shutdown now cancels MCP operations and waits for its owned HTTP server before flushing persistence.
- Preferences migration persists its completion marker across fresh handles, preserves malformed legacy files, and rolls back a settings update if its rollback JSON mirror cannot be written.
- Repeated CSV headers at the maximum identifier length retain a unique suffix instead of looping during column inference.

- Editing a long text or JSON cell in the grid now starts from its full value instead of the shortened text shown for display, so a small edit no longer truncates the rest of the value.
- The server activity view shows a binary value as its byte count instead of a label that read like a hexadecimal escape.
- The structure editor shows a column default as the SQL that defines it, so an empty-string default no longer reads as no default, and a MySQL column edit no longer drops an empty-string default or fails on a text default.
- On X11 the app's windows now group under its launcher in docks and task switchers, whichever command started it.
- Changing a PostgreSQL column's type in the structure editor keeps the column's own collation instead of resetting it to the type default, and a MySQL column edit keeps its collation instead of taking the table default.
- A database driver that stops unexpectedly is reported as a failed operation, with writes marked as having an unknown result, instead of leaving the request waiting forever. The connection is then replaced rather than reused.
- Embedded application icons now use the same BookiE artwork as installed packages.
- Malformed MongoDB SCRAM nonces are rejected instead of crashing authentication.
- SSH key and TLS certificate reads remain bounded if a file changes while it is read.
- SSH socket forwarding falls back to a short private path when the runtime directory would exceed Unix limits.
- ClickHouse connection timeouts no longer misreport a certificate hostname mismatch.
- MongoDB server-selection failures (for example, an unreachable host) no longer misreport a certificate hostname mismatch.
- A SQL Server script now keeps running the next `GO` batch after one batch fails, instead of stopping the whole script.
- PostgreSQL indexes built on an expression now appear in the structure tab with the expression as their key, a partial index shows its WHERE condition, and INCLUDE columns are no longer listed as index keys.
- SQLite foreign keys that reference the parent table without naming its columns now show the parent's primary key columns instead of blanks.
- SQLite foreign keys, indexes and columns read right after a structure change are no longer missing when the read lands on another pooled connection.
- A SQL Server batch that raises an error after its first result set now reports the error instead of showing the first result as a success.
- A SQL Server result cut at the row limit now waits for the rest of the batch to finish, instead of leaving it running on the server with its locks held until the next query.
- MongoDB accepts `DROP TABLE people` as well as the quoted form, instead of reporting the statement as unsupported.
- Renaming a column and changing nothing else now saves. On SQLite the whole save was refused, and on MySQL the column definition was restated without its collation, character set or comment.
- MySQL and SQL Server values that the driver cannot decode now show as undecodable instead of as an empty cell. Such a cell stays read-only, and a row whose key could not be read is refused rather than updated or deleted silently.
- A PostgreSQL result with one undecodable value (for example, a NUMERIC too wide to represent) shows that cell as undecodable, logs the column so the cause can be traced, and keeps the rest of the row and result instead of failing the whole query. An undecodable cell stays read-only, and a row whose key could not be read is refused instead of being changed or deleted silently.
- Redis binary values with invalid UTF-8 retain their original bytes. SQL statement export preserves binary values using exact literals on PostgreSQL, MySQL, SQLite, SQL Server, ClickHouse and DuckDB; unsupported binary exports, undecodable values and non-finite numbers are refused instead of substituting NULL. Copy as IN continues to skip binary and undecodable cells.
- CSV export and Copy as CSV write empty text as a quoted empty field and NULL as a bare empty field, so the two stay distinct. With quoting set to Always, NULL was also written as a quoted empty field.
- SQL Server INSERT statements copied or exported from a `datetime` column with milliseconds now run and restore the same value. SQL Server rejected them with a conversion error.
- ClickHouse decimals keep their declared scale, so 2.50 in a Decimal(10, 2) column no longer shows as 2.5.
- ClickHouse timestamp parameters keep their fractional seconds. Local timestamps lost every fraction and timezone timestamps were cut to milliseconds.
- ClickHouse SQL INSERT exports of timezone timestamps now run and keep every fractional digit. ClickHouse rejected them with a parse error.
- ClickHouse `DateTime64(9)` parameters and SQL exports outside its exact nanosecond range are refused locally instead of reaching an engine range failure or conversion.
- ClickHouse `DateTime` and `DateTime64` results with named IANA timezones now preserve their instant. Ambiguous or nonexistent local clock values are shown as undecodable rather than guessed.
- SQL Server `datetimeoffset` values keep their stored instant, original offset and all seven fractional digits in the grid, in edits and in copied or exported INSERT statements. A non-UTC value was shifted by its offset a second time, and every value was converted to UTC.
- MySQL and MariaDB INSERT statements copied or exported from text or JSON holding a backslash or NUL character now restore the same value when the server runs with `NO_BACKSLASH_ESCAPES`. Each backslash was doubled, or the statement was rejected.
- MySQL column-comment DDL refuses comments containing backslashes until it can execute with a known session SQL mode, rather than silently storing a different comment.
- MySQL `BIT` columns show their numeric value, supported widths can be edited within range, and spatial columns show their stored geometry bytes as read-only values; copied or exported INSERT statements restore the same data. These columns were shown as empty or unreadable cells, or could not be edited safely.

### Security

- Unparseable statements require approval when write or DDL approval is enabled, even when unparseable SQL is otherwise allowed.
- Statements that access host files or run external programs, writes with unknown effects, and Local `TRUNCATE` require human approval.
- The agent daemon no longer trusts an SSH host key it has not seen before. An unattended connection to an unknown host fails and names the key's fingerprint; connecting once from the app or with `ssh` records it. A changed key is still refused everywhere.
- A lone BEGIN, COMMIT or ROLLBACK on a shared connection is refused with an explanation, in the SQL editor and through MCP. Each statement ran on a shared connection, so a script such as `BEGIN; UPDATE …; ROLLBACK;` committed the update while reporting every step as successful. A whole transaction sent as one batch, such as a SQL Server `GO` batch, still runs.
- A saved connection's SSH jump chain is capped at eight hops, so an edited connection file cannot force a deep recursive parse.

## [0.1.4] - 2026-09-17

### Changed

- Isolated GTK and Secret Service tests through shared local/CI runners.
- Arch and Debian packages: system SQLite, Rust 1.98, and `bookie` launchers with `tablepro` aliases.

### Fixed

- SQL formatting dropping significant whitespace and literal prefixes.
- BOOLEAN cells with mismatched bytes/text entering the inline editor.
- PostgreSQL INTERVAL overflow and malformed INET/CIDR prefixes treated as NULL.
- DuckDB identifier quoting and Redis read-only SELECT in the manual connection lab.
- Debian fallback package installing `tablepro` while the desktop file launched `bookie`.
- ClickHouse VerifyFull reporting a hostname mismatch as a dropped connection.
- MongoDB VerifyFull reporting an IP-endpoint identity mismatch as connection refused.
- Redis VerifyFull reporting an IP-endpoint identity mismatch as connection refused.

### Security

- Agentd session reuse after Secret Service or SSH/CA file rotation.
- Unbounded PostgreSQL and MongoDB SCRAM iteration counts.

[Unreleased]: https://github.com/cozyGarage/BookiE/compare/linux-v0.2.5...linux
[0.1.5]: https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.1.5
[0.2.0]: https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.0
[0.2.2]: https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.2
[0.2.3]: https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.3
[0.2.5]: https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.5
