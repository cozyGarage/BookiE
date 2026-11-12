# Connection handling

[ADR 0008](decisions/0008-connection-and-session-ownership.md) owns identity and trust; [ADR 0005](decisions/0005-server-side-cancellation.md) owns cancellation. This guide describes the connection contracts.

## Shared assembly and ownership

GUI and agentd use `bookie-transport` to resolve saved configuration/secrets, assemble options, open the selected SSH route, and connect the driver. Consumers receive `PolicyGuard` handles. MCP scopes and connection allowlists apply in addition to SQL policy. Dedicated editor sessions retain their own physical identity; loss refuses subsequent SQL rather than using a pool or replaying it.

Saved UUID, live handle, editor session and namespace are distinct owners. The GUI monitor watches tunnel closure and driver fault signals as well as ping; replacement changes identity and invalidates old editor sessions. Stop visibility uses the driver's capability, not an engine-name list. Actual loss/cancellation outcomes and test ownership are in [disconnection contracts](disconnection-contracts.md).

GUI and agentd scope connection state to the live generation. Agentd creates an `AuditState` generation for each cached session and attaches a fault sink that retires only the matching session, even if its ping still succeeds. Journal failure remains shared across sessions.

After a connection attempt is classified, permanent login and database errors stop the reconnect loop: authentication failures (class 28, including a wrong password, unknown role or no `pg_hba.conf` entry) and a missing database (3D000) are not retried. A busy server (53300) and network faults remain retryable. PostgreSQL's initial SQLx pool startup can retry connection attempts until its acquire deadline before the driver returns an error; this is separate from the app's reconnect classification.

## TLS identity

The dial endpoint and authenticated service hostname are separate. SSH retains the saved service identity. Where supported, verifying connections forward a private Unix socket; other drivers must explicitly verify the original hostname over TCP or refuse. Local PostgreSQL sockets use a socket directory plus port. MySQL and MariaDB local sockets use the full socket-file path. Local sockets reject SSH and TLS; forwarded sockets retain the remote identity. Selecting a local socket never falls back to TCP.

Saved custom CA and client certificate/key paths are assembled through saved connections, bundles, GUI setup and agentd transport. PostgreSQL and MySQL drivers accept configured client identities; other drivers refuse them. TLS modes that can fall back to plaintext are rejected, and missing, partial, non-file or oversized identity material fails closed.

MongoDB and Redis TLS backends have no CA-only mode, so `VerifyCa` also verifies the hostname. PostgreSQL and MySQL Verify CA and Verify Full use the operating system trust store when no custom authority is named. A named authority is added to those roots. Verify CA checks the certificate chain; Verify Full also checks the service hostname. Typed keyring failure propagates instead of becoming a missing password; see [storage](storage.md#secrets).

## SSH jump hosts

The SSH section of the connection form edits a chain of hosts. The first host uses the rows of the section. **Add jump host** appends further hosts in the order BookiE connects through them; each has its own host, port, user and authentication (password, private key or ssh-agent). The up and down buttons reorder jump hosts, and the trash button removes one. A chain has at most eight hosts, and a host cannot appear twice with the same name and port.

Passwords and key passphrases of jump hosts go to Secret Service, one entry per host and per credential revision, never into `connections.json`. When a saved connection is opened for editing the form shows whether a secret is stored and never its value; leave the field empty to keep it. Typing a new value stores it under the next revision, and the replaced entry is deleted once the connection is saved. If the save fails, the new entries are removed and the old ones stay. Removing a jump host deletes its entries. The first host keeps the older per-connection entries. Each host keeps its identity across edits, so saved trust decisions and secrets stay attached to it.

The system OpenSSH client refuses saved jump hosts, so the form disables the jump host list while that client is selected and refuses a save that keeps jump hosts with it.

## Built-in SSH trust

The GUI defaults to `UnknownHostKey::Refuse` and installs a prompter. An unknown key requires explicit consent before persistence; decline/cancel must write no key. A changed key is refused. Each hop owns its own trust decision. Agentd is unattended and refuses unknown keys. Built-in known hosts are distinct from OpenSSH's `~/.ssh/known_hosts`; running system `ssh` does not populate the built-in store. Manual scenarios are listed in the [verification checklist](manual-verification-0.2-features.md).

## System OpenSSH

A saved tunnel can select `client: open_ssh`. Its ControlMaster/forward lives in a private runtime directory; `StrictHostKeyChecking=ask` applies to the destination and every `ProxyJump` hop. GUI prompts explicitly and agentd declines unanswered prompts. Saved passwords/passphrases answer only a matching destination/key prompt. Jump routing comes from `~/.ssh/config`/`ProxyJump`; effective `ProxyCommand` routes are refused. Saved per-hop chains are refused for this backend. Built-in SSH supports saved chains and ssh-agent.

Inside Flatpak, selecting system OpenSSH fails with an explicit message before resolving saved SSH credentials or dispatching the database driver. The client does not switch to built-in SSH; select that backend explicitly if it is wanted. Native system OpenSSH remains available outside Flatpak. The executable `bookie-askpass` helper must be installed beside the app or on PATH. Debian packages install it with the application binaries.

## Daemon session cache and DuckDB flat files

A `bookie-agentd` session key includes the saved transport fields plus a digest of Secret Service secret values and SSH private-key, TLS CA, and TLS client certificate and private-key contents. Rotating any of them opens a new session. Handles already issued keep the session they hold until released. If a secret or material file cannot be read, a new open is refused instead of serving the cache.

Read-only DuckDB connections accept a database file or a selected CSV, TSV, Parquet or JSON file. A flat-file connection exposes the selected input as a view in an in-memory database. The driver pins the selected regular file, creates a private alias on the same filesystem and allows DuckDB to read only that alias; setup fails closed if pinning cannot be applied. Read-only database files open without write access and with external-file access disabled. SQL writes and read-only in-memory connections are refused. These are DuckDB-level path restrictions, not an operating-system sandbox.
