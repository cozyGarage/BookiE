# Manual verification: connections and SSH

Part of the [manual verification checklist](manual-verification-0.2-features.md), which holds the setup and private recording rules for every item here.

## Connection dialog and browse shortcuts

- [ ] Screen readers can activate Driver, Endpoint and TLS, and announce Port, Connect timeout and Query timeout. The sidebar search, saved-filter name and list colour chooser are outside this connection-form check.
- [ ] New connection defaults match the selected driver; no PostgreSQL defaults leak into another engine. Switching drivers and editing a saved connection preserves intended custom values.
- [ ] For MySQL/MariaDB, select a local Unix socket and confirm the form shows a socket-file path while hiding host and port. Switch back to Network and confirm the socket path is hidden and host/port return.
- [ ] Connect through a real local MySQL/MariaDB socket, save the connection, reopen it, and confirm the socket path and endpoint mode remain selected. Export and re-import a bundle and confirm the same endpoint is retained.
- [ ] Confirm a missing or invalid socket path fails without attempting a TCP connection. Local sockets refuse TLS and SSH; existing PostgreSQL socket-directory and SSH-forwarded MySQL socket connections still work.
- [ ] The current header title and grouped Test/Connect actions fit at narrow width in light and dark; the title does not overlap the actions.
- [ ] Enter submits the form and Escape dismisses without changing saved data.
- [ ] For SQLite and DuckDB, use Create a new database file with a new name: the file appears on disk as an empty file readable and writable only by its owner, the path fills the File path row, and Connect opens it. Choosing an existing file in the same dialog leaves the file unchanged and shows a message that it was not replaced. Delete the file of a saved connection and connect: the message says the database file does not exist and no file is created.
- [ ] Browse shortcuts show the actual Left, Right, Space and pointer modifiers; keyboard and click actions match their labels.

## Production marker

- [ ] Connect to a connection marked Production: the header bar is tinted red, a red **Production** badge shows beside Read-only, and the window title starts with `[Production]` in the taskbar or Alt-Tab. Open a table and change a cell: the title reads `• [Production] …`. The badge text and the tinted bar are readable in light and dark.
- [ ] Disconnect, or connect to a Dev, Staging or Local connection: the badge, the tint and the title prefix are gone.
- [ ] A Production connection that is also Read-only shows both badges.

## Connection list

- [ ] Connections are grouped. Nested paths such as `Prod / EU` show as a hierarchy; each folder can collapse and expand. Ungrouped connections stay in their own section.
- [ ] A nested path that appears twice still renders one folder at each level.
- [ ] The colour swatch shows on a tagged row and the picker sets it.
- [ ] After connecting to a tagged connection, the workspace tab strip shows its colour. Changing or clearing the tag updates the strip, and disconnecting clears it.
- [ ] With Orca, open Group and tags, navigate to Colour, open the named button, choose a palette colour, and confirm the selected colour is announced and saved. Sidebar table search and the saved-filter name field announce their purpose.
- [ ] Duplicate produces a new row with a distinct name. When the source has a saved database password, the copy's credential form opens with an empty password field; save a new password and verify both connections work independently.
- [ ] The Name field in the connect dialog is used when set, and the derived `user@host` label still appears when it is blank.
- [ ] Two connections to the same endpoint stay separate after reconnecting to each.
- [ ] For MongoDB, the connect dialog shows Authentication database; save `admin` for a user defined there while Database is `appdb`, reconnect, and confirm connections with different authentication databases stay separate.
- [ ] On PostgreSQL, right-click a schema and hide it. Confirm its tables and views disappear, search cannot reveal them, the filter menu can restore one or all schemas, and each connection retains its own hidden-schema list after reconnecting.
- [ ] Open two saved connections in one window, select a tab of the first one, and quit. Start BookiE: both connections reopen in the same window with their tabs, and the connection that was in front is in front again. Stop one of the two database servers, quit and start again: the reachable connection reopens and the other reports Connection failed in the same window.
- [ ] With two connections open, disconnect both and quit. Start BookiE: the welcome page appears and nothing reconnects.
- [ ] With connection A in one window and connection B in a second window, quit. Start BookiE: both windows reopen with their own connection. Delete connection B from the saved list, quit and start again: only A reopens and no empty second window appears. Open A and B in two windows again, close the second window with its close button, then quit: only A's window reopens. Open both windows again and quit with Ctrl+Q while both are open: both windows reopen.
- [ ] Open connections A and B in one window, open a table tab of each, and edit a cell in the A tab. Select the B tab, then close the A tab with its close button and choose Save: the change reaches A's database only and B's database is unchanged. Run the same check with a structure change.
- [ ] With A and B open and an unsaved edit in a tab of each, close the window. One dialog names both connections. Cancel keeps both open with the edits. Close again and choose Save: each tab saves to its own database and the window closes. Make the save of one fail (stop its server): the window stays open and a toast names that connection.
- [ ] With A and B open and an unsaved edit in a tab of each, choose Disconnect while A is in front: the prompt mentions A only, B and its edits stay open, and only A's tabs close. An open transaction in a B editor is not rolled back.
- [ ] With A open, try to open an unreachable saved connection C: an error dialog appears in the same window, A and its tabs are untouched and no window opens.
- [ ] With A and B open, run an MCP client against B only. Quit and start BookiE: the record restores A and B as the window held them, and no connection appears that was only opened for the client.

## Long-query notice

- [ ] Run `SELECT pg_sleep(12)`, switch to another window: a "Query finished" notification appears when it ends. With the window focused, none appears.

## Per-connection timeouts

- [ ] The connect dialog shows Connect timeout and Query timeout rows; 0 means default.
- [ ] A connection saved with a 2-second query timeout stops `SELECT pg_sleep(5)` after about 2 seconds, while another connection still uses the Preferences timeout.
- [ ] A connect timeout of 3 seconds to an unreachable host fails after about 3 seconds.

- [ ] With a 2-second query timeout, run `SELECT pg_sleep(5)` on PostgreSQL (streamed) and `SELECT SLEEP(5)` on MySQL (materialized): after each, the toolbar shows **Run again (8s timeout)**. Click it: the same query runs and completes. Running any other query removes the button, and the saved timeout is still 2 seconds.
- [ ] Edit the editor text after a timeout, then click the button: the query that timed out runs, not the new text.

## Connection bundles

- [ ] Export without a passphrase, then read the file: it contains hosts and usernames and **no** password, passphrase or token.
- [ ] Export with a passphrase, then import it on a second profile (`BOOKIE_PROFILE=development`) and confirm the connections and their credentials arrive.
- [ ] A wrong passphrase and a file edited after export give the **same** message.
- [ ] Importing a bundle whose id already exists locally against a different server creates a new connection and leaves the local one untouched.
- [ ] The import plan lists every connection with its disposition and can be unticked before anything is written.

## DBX connection import

- [ ] Select a DBX `dbx.db` file. Supported direct connections appear as editable drafts with credentials empty; unsupported engines and SSH/proxy connections appear in the skipped list.
- [ ] Confirm the source database and its `connection_secrets` rows are unchanged after previewing and cancelling the import.
- [ ] Confirm saved SQL is not imported.

## Built-in SSH host keys and tunnel loss

- [ ] A new host shows its fingerprint before trust; decline/cancel writes no key and opens no database connection. Explicit acceptance persists only that key.
- [ ] A changed key is refused. Reconnect never silently learns an unknown key.
- [ ] Bastion loss retires affected editor sessions and starts reconnect promptly; a result from the old session cannot replace current state.

## SSH jump hosts

- [ ] In the SSH section choose **Add jump host** twice and fill the three hosts (first host in the section, then two jump hosts) with one password host and one key host. Connect through the chain and run a query.
- [ ] Reopen the connection for editing. All hosts appear, no secret is shown, a password field says it is saved and an untouched save keeps the connection working.
- [ ] Move a jump host up and down, remove one and save: the chain connects in the new order and the removed host's keyring entry is gone (`secret-tool search` shows no entry for its host).
- [ ] Type a new password for a jump host and save: the connection works with the new password and only one entry remains for that host.
- [ ] Turn on "Use system OpenSSH": the jump host list is disabled and says why. With jump hosts present, saving is refused with a message.
- [ ] Duplicate a connection with jump hosts: the copy opens for editing, shows the hosts and asks for their passwords.

## SSH agent (built-in client)

- [ ] Choose "SSH agent" in the SSH section with a key loaded in ssh-agent: the connection authenticates without a password or key file prompt.
- [ ] With several keys in the agent, fill "Agent key file (optional)" with the public key file of the right one (`~/.ssh/<name>.pub`) and save: the connection offers only that key, and reopening the connection shows the path again. Clear the field to offer every agent key.
- [ ] With the agent stopped, the error says the agent could not be reached.

## System OpenSSH tunnel

Build and install with the helper (`cargo build -p bookie-ssh --bin bookie-askpass` beside the app binary, or an installed package).

- [ ] Turn on "Use system OpenSSH" in the SSH section and connect to a host already in `~/.ssh/known_hosts`: no prompt, the connection works.
- [ ] Connect to a new host: a "Trust this SSH host?" dialog shows the fingerprint; Cancel fails the connection and writes nothing.
- [ ] A host reached through a `ProxyJump` line in `~/.ssh/config` connects.
- [ ] A new key at a `ProxyJump` hop requires the same explicit approval as a new destination key; declining does not add the jump key to `known_hosts`.
- [ ] A saved password is used for the target host; a jump host asking for a password gets a dialog instead of the saved one.
- [ ] A PostgreSQL connection with Verify Full through the OpenSSH tunnel verifies the database hostname.
- [ ] After closing the app, no `ssh -M` process is left running.
- [ ] On a non-Linux target, selecting system OpenSSH reports that the backend is unavailable and directs the user to the built-in client.

## SQLite attached databases

- [ ] When attachment editing is available in connection settings, add one or more SQLite files with schema aliases and paths (including `~/` expansion). On connect, verify each schema appears in the sidebar. The current form has no add or edit control.
- [ ] Cross-schema queries (such as `SELECT * FROM main.orders JOIN aux.customers ON orders.customer_id = aux.customers.id`) execute in the SQL editor and return combined rows.
- [ ] Connection export and import preserve configured attached database definitions across profiles.

