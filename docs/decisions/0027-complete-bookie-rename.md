# 0027: Complete the BookiE rename and copy legacy state forward

- **Status**: Accepted (2026-11-01)
- **Date**: 2026-11-01
- **Supersedes**: [0009](0009-persistence-and-identity-compatibility.md) where it keeps `tablepro` identifiers

## Context

ADR 0009 kept the `tablepro` crate names, environment variables, application ID, XDG directories, keyring schema and file-format identifiers after the product was renamed. The maintainer approved the explicit migration that ADR required, so the legacy name can go.

## Decision

Every identifier is `bookie` or `com.bookie.linux`: Cargo packages, crates and binaries, environment variables, application ID, GSettings schema and path, resource path, XDG directory names, keyring schema, connection bundle format, MCP server name and package names. The only remaining `tablepro` strings are upstream attribution, historical records and the legacy names listed below.

Existing installations keep their data through copy-forward, never rewrite:

- On start, the app and `bookie-agentd` copy `tablepro` and `tablepro-devel` under the config, data and state directories to `bookie` and `bookie-devel`. The copy is staged and renamed into place, skips symlinks, preserves permissions, and does nothing when the new directory exists. The legacy directory is left for rollback.
- A credential missing under the `bookie` keyring schema is read from the legacy `com.tablepro.linux` schema and written under the new one. Deleting a credential removes both copies.
- Connection bundles exported under `tablepro.connection-bundle` still import and decrypt. New exports use `bookie.connection-bundle`.
- Preferences migrate through the existing `preferences.json` mirror. GSettings values stored only under the old schema path are not carried over.
- The audit journal is copied, not rewritten.
- The Arch and Debian packages replace and conflict with the legacy `tablepro` package. No `tablepro` command aliases are installed.

## Consequences

Flatpak installs keep their data under `~/.var/app/com.tablepro.linux`, which the new application ID cannot read; moving it is a manual step. MCP clients and scripts that call `tablepro` or `tablepro-agentd` must switch to `bookie` and `bookie-agentd`. Runner environments that export `TABLEPRO_*` variables must export `BOOKIE_*`. The legacy copy and keyring fallback can be removed in a later release once supported upgrade paths no longer start from a `tablepro` build.
