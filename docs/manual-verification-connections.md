# Manual verification: connections and SSH

Part of the [manual verification checklist](manual-verification-0.2-features.md), which holds the setup, evidence and recording rules that apply to every item here.

## Connection dialog and browse shortcuts

- [x] Screen readers can activate Driver, Endpoint and TLS, and announce Port, Connect timeout and Query timeout. The sidebar search, saved-filter name and list colour chooser are outside this connection-form check; the specific missing-name/action report in [#658](https://github.com/cozyGarage/BookiE/issues/658) was rechecked and closed.
- [x] New connection defaults match the selected driver; no PostgreSQL defaults leak into another engine. Switching drivers and editing a saved connection preserves intended custom values.
- [x] For MySQL/MariaDB, select a local Unix socket and confirm the form shows a socket-file path while hiding host and port. Switch back to Network and confirm the socket path is hidden and host/port return.
- [ ] Connect through a real local MySQL/MariaDB socket, save the connection, reopen it, and confirm the socket path and endpoint mode remain selected. Export and re-import a bundle and confirm the same endpoint is retained.
- [ ] Confirm a missing or invalid socket path fails without attempting a TCP connection. Local sockets refuse TLS and SSH; existing PostgreSQL socket-directory and SSH-forwarded MySQL socket connections still work.
- [ ] The current header title and grouped Test/Connect actions fit at narrow width in light and dark; the title does not overlap the actions.
- [x] Enter submits the form and Escape dismisses without changing saved data.
- [ ] Browse shortcuts show the actual Left, Right, Space and pointer modifiers; keyboard and click actions match their labels.

## Production marker

- [x] Connect to a connection marked Production: the header bar is tinted red, a red **Production** badge shows beside Read-only, and the window title starts with `[Production]` in the taskbar or Alt-Tab. Open a table and change a cell: the title reads `• [Production] …`. The badge text and the tinted bar are readable in light and dark.
- [x] Disconnect, or connect to a Dev, Staging or Local connection: the badge, the tint and the title prefix are gone.
- [x] A Production connection that is also Read-only shows both badges.

## Connection list

- [x] Connections are grouped, with one section per group and an ungrouped section.
- [ ] A group that appears twice in the ordering still renders one section.
- [x] The colour swatch shows on a tagged row and the picker sets it.
- [ ] After connecting to a tagged connection, the workspace tab strip shows its colour. Changing or clearing the tag updates the strip, and disconnecting clears it. The GTK regression passes; no installed retest after [#768](https://github.com/cozyGarage/BookiE/pull/768) is recorded here.
- [x] With Orca, open Group and tags, navigate to Colour, open the named button, choose a palette colour, and confirm the selected colour is announced and saved. Sidebar table search and the saved-filter name field announce their purpose. No native Wayland screen-reader result is recorded here.
- [x] Duplicate produces a new row with a distinct name. When the source has a saved database password, the copy's credential form opens with an empty password field; save a new password and verify both connections work independently.
- [x] The Name field in the connect dialog is used when set, and the derived `user@host` label still appears when it is blank.
- [ ] Two connections to the same endpoint stay separate after reconnecting to each. This is the regression the id-first identity change fixed.
- [x] For MongoDB, the connect dialog shows Authentication database; save `admin` for a user defined there while Database is `appdb`, reconnect, and confirm connections with different authentication databases stay separate. **Not run.**
- [x] On PostgreSQL, right-click a schema and hide it. Confirm its tables and views disappear, search cannot reveal them, the filter menu can restore one or all schemas, and each connection retains its own hidden-schema list after reconnecting.

## Long-query notice

- [x] Run `SELECT pg_sleep(12)`, switch to another window: a "Query finished" notification appears when it ends. With the window focused, none appears.

## Per-connection timeouts

- [x] The connect dialog shows Connect timeout and Query timeout rows; 0 means default.
- [x] A connection saved with a 2-second query timeout stops `SELECT pg_sleep(5)` after about 2 seconds, while another connection still uses the Preferences timeout.
- [x] A connect timeout of 3 seconds to an unreachable host fails after about 3 seconds.

- [ ] With a 2-second query timeout, run `SELECT pg_sleep(5)` on PostgreSQL (streamed) and `SELECT SLEEP(5)` on MySQL (materialized): after each, the toolbar shows **Run again (8s timeout)**. Click it: the same query runs and completes. Running any other query removes the button, and the saved timeout is still 2 seconds.
- [x] Edit the editor text after a timeout, then click the button: the query that timed out runs, not the new text.

## Connection bundles

- [x] Export without a passphrase, then read the file: it contains hosts and usernames and **no** password, passphrase or token.
- [ ] Export with a passphrase, then import it on a second profile (`TABLEPRO_PROFILE=development`) and confirm the connections and their credentials arrive.
- [x] A wrong passphrase and a file edited after export give the **same** message.
- [ ] Importing a bundle whose id already exists locally against a different server creates a new connection and leaves the local one untouched.
- [x] The import plan lists every connection with its disposition and can be unticked before anything is written.

## Built-in SSH host keys and tunnel loss

- [ ] A new host shows its fingerprint before trust; decline/cancel writes no key and opens no database connection. Explicit acceptance persists only that key.
- [x] A changed key is refused. Reconnect never silently learns an unknown key.
- [ ] Bastion loss retires affected editor sessions and starts reconnect promptly; a result from the old session cannot replace current state.

## SSH agent (built-in client)

- [x] Choose "SSH agent" in the SSH section with a key loaded in ssh-agent: the connection authenticates without a password or key file prompt.
- [x] With the agent stopped, the error says the agent could not be reached.

## System OpenSSH tunnel

Build and install with the helper (`cargo build -p tablepro-ssh --bin tablepro-askpass` beside the app binary, or an installed package).

- [ ] Turn on "Use system OpenSSH" in the SSH section and connect to a host already in `~/.ssh/known_hosts`: no prompt, the connection works.
- [x] Connect to a new host: a "Trust this SSH host?" dialog shows the fingerprint; Cancel fails the connection and writes nothing.
- [x] A host reached through a `ProxyJump` line in `~/.ssh/config` connects.
- [x] A new key at a `ProxyJump` hop requires the same explicit approval as a new destination key; declining does not add the jump key to `known_hosts`.
- [ ] A saved password is used for the target host; a jump host asking for a password gets a dialog instead of the saved one.
- [x] A PostgreSQL connection with Verify Full through the OpenSSH tunnel verifies the database hostname.
- [x] After closing the app, no `ssh -M` process is left running.

## SQLite attached databases

- [ ] When attachment editing is available in connection settings, add one or more SQLite files with schema aliases and paths (including `~/` expansion). On connect, verify each schema appears in the sidebar. The current form has no add or edit control.
- [x] Cross-schema queries (such as `SELECT * FROM main.orders JOIN aux.customers ON orders.customer_id = aux.customers.id`) execute in the SQL editor and return combined rows.
- [ ] Verify attachment failures are surfaced when implemented. Missing paths are currently skipped, and errors attaching existing paths are ignored; [#1993](https://github.com/cozyGarage/BookiE/issues/1993) tracks the latter.
- [ ] Connection export and import preserve configured attached database definitions across profiles.

