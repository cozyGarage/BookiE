# PostgreSQL restricted-role catalog visibility

## Scope

This probe runs SQL matching BookiE's current PostgreSQL catalog query pattern for a role with access to one schema and no access to another. It tests metadata visibility only. It does not execute the Rust driver or demonstrate access to table rows or execution of an inaccessible routine.

## Source and environment

- BookiE `linux` source: `a41c18389f638d044d98f93d9f6ed3514ceb2a3d`.
- PostgreSQL image: `postgres:16-alpine`, image ID `sha256:721873c34ceb9f8d8fc265984940dc982404c105f19ad51be9fdc5970a6080ea`, PostgreSQL 16.15.
- The container used `--network none` and was removed after the probe.
- The relevant table query in `crates/drivers/postgres/src/lib.rs` selects from `pg_catalog.pg_tables` and excludes only `pg_catalog` and `information_schema`.
- The catalog query builder in `crates/drivers/postgres/src/catalog.rs` excludes those same system schemas; routine metadata is selected from `pg_catalog.pg_proc`.

## Setup

The fixture created role `catalog_limited`, schemas `bookie_visible` and `bookie_hidden`, a table in each schema, and `bookie_hidden.hidden_function()`. It granted the limited role `USAGE` on `bookie_visible` and `SELECT` on its table only. The probe then changed to that role before querying catalogs.

`probe.sql` contains the setup and queries. It was run with `psql -U postgres -d postgres -v ON_ERROR_STOP=1 -f probe.sql` inside the isolated container.

## Observed output

```text
pg_tables=bookie_hidden.denied_rows,bookie_visible.allowed_rows
information_schema=bookie_visible.allowed_rows
privileges=false,false,true,true
pg_proc=bookie_hidden.hidden_function
```

The privilege values are schema `USAGE` and table `SELECT` for the hidden schema/table, followed by the same checks for the visible schema/table. The two hidden-object names appear in system-catalog queries even though both privilege checks are false. `information_schema.tables` returned only the accessible table.

## Finding and limits

The production `list_tables` query has the same catalog source and system-schema predicate as the probe's `pg_tables` query, so it will include tables in schemas where the connected role has no schema or table privileges. The catalog routine query reads the same `pg_proc` and namespace catalogs used by the probe, so it can return a routine name from such a schema. This is a source-based inference from the matching SQL and observed PostgreSQL results; the driver wrapper itself was not exercised. The result may make the sidebar and catalog views suggest that inaccessible objects are usable.

This is not a database authorization bypass: PostgreSQL still denies access to the hidden schema and table, and this probe did not read hidden table rows or call the hidden routine. The result is limited to PostgreSQL 16.15 and the queries above. It does not establish behavior for every relation kind, grant combination, server version, MCP caller or statistics path.

The remediation crosses the B3 driver/catalog boundary and the UX catalog scoping path. Any change should first define whether metadata visibility follows schema `USAGE`, object privileges, the server's catalog visibility rules, or a combination, then pin the chosen behavior with restricted-role integration cases.

## Recovery verification, October 26

The SQL probe was rerun in a new disposable container using the same image ID above, with networking disabled and no published ports. All four recorded catalog and privilege results matched. The container was removed afterward. This rerun verifies the standalone SQL behavior; it does not execute the Rust driver wrapper.
