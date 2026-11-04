# 0034: Metadata visibility for restricted PostgreSQL roles

- **Status**: Proposed (2026-11-04)
- **Date**: 2026-11-04

## Context

Issue #580 asks for one stated rule before any PostgreSQL catalog SQL changes. Today `list_tables` reads `pg_tables` for every schema except `pg_catalog` and `information_schema` (`crates/drivers/postgres/src/lib.rs`). The `pg_*` catalog views list objects whatever the caller's privileges, so a role with access to `bookie_visible` and none to `bookie_hidden` still gets `bookie_hidden` tables in the sidebar, completion and any tool that enumerates the catalog. Reads of their columns, keys or row counts then fail or come back empty, which risks showing invented or missing facts. `information_schema` filters by privilege; `pg_catalog` does not.

Large multi-tenant catalogs add a cost problem: every schema's tables are listed and then read, although the user may hide most schemas (the per-connection schema filter exists, but catalog queries are not scoped by it).

## Decision

Not yet decided. This record asks the maintainer to choose the rule, then B3 to hand off the SQL change.

## Options

- **A. Show only reachable objects.** Keep a schema when `has_schema_privilege(schema, 'USAGE')` holds and a relation when `has_any_column_privilege(relation, 'SELECT,INSERT,UPDATE,REFERENCES')` holds. One rule for the sidebar, completion, ER diagram, schema compare, MCP and statistics tools. Cost: a user cannot see that an object exists but is denied.
- **B. Show everything and mark the unreachable.** List as today, mark objects the role cannot reach (dimmed, not openable) and never read or invent metadata for them. Cost: more code in every consumer, and the hidden names still appear.
- **C. Keep today's behaviour.** Cost: the invented-fact risk in the context stays.

## Recommendation

A, with the privilege test in SQL so the cost is one catalog query, not a request per table. Add the per-connection hidden-schema set to the catalog queries so hidden schemas are never listed or read. Preserve exact identifiers, session context and stale-result refusal.

## Consequences

- The PostgreSQL driver and catalog SQL are B3-owned: the change needs the handoff and the lane, project and index update that `AGENTS.md` requires.
- Acceptance evidence is a real PostgreSQL fixture with a restricted role (`bookie_visible` reachable, `bookie_hidden` not) checked through every consumer, plus a release-tier many-schema run recording catalog latency, main-loop stalls and memory at a named commit. Synthetic measurements stay separate.
- Until a rule is accepted, no catalog SQL changes.
