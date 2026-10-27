# Structure metadata capability acceptance, 2026-10-26

Issue [#1688](https://github.com/cozyGarage/BookiE/issues/1688), PR [#1695](https://github.com/cozyGarage/BookiE/pull/1695).

## Source and environment

- Tested implementation: `80f54cab96727ef4dde2f41d208d5660694d9541`. The following evidence commit changes documentation only.
- Fresh optimized `ci` build, copied by `scripts/gtk-test-binary.sh` into its fixture install layout. Binary digest: [binary.sha256](binary.sha256). Relevant source digests: [sources.sha256](sources.sha256).
- Arch/Omarchy host, isolated X11/Xvfb, Cairo renderer, private D-Bus, AT-SPI and Secret Service. System Python provides `pyatspi`.
- PostgreSQL 17.11 from `postgres:17-alpine`; ClickHouse fixture image `clickhouse/clickhouse-server:24.8`.
- These are fixture-installed app runs. System package installation, native Wayland, distro-floor and Forgejo acceptance are not proved here.

## Given / When / Then

Given a PostgreSQL table with no indexes or foreign keys, when Structure opens each metadata section, then Add actions remain available and no unavailable notice appears. Given another table with an actual index and FK, when its sections open, then the database-provided names render. The disposable schema is created and removed through PostgreSQL with error checking.

Given a ClickHouse table with supported index metadata and unsupported FK metadata, when Structure opens, then the index section has no unavailable notice, while the FK section explains the unsupported read. The existing Add action remains available; metadata-read capability does not grant or deny DDL mutation support.

The first screenshot review found extra dimming on the informational row. The final implementation uses normal ActionRow text contrast. The [PostgreSQL capture](postgres-structure-foreign-keys.png) and [ClickHouse capture](clickhouse-structure-unavailable-foreign-keys.png) were inspected after rebuilding that change.

## Commands and results

Run from the repository root with system Python and Cargo on PATH:

```bash
PATH="/usr/bin:/bin:$HOME/.cargo/bin" TABLEPRO_GTK_SCENARIO=postgres_structure_preserves_empty_and_populated_metadata bash scripts/test-gtk-postgres.sh
PATH="/usr/bin:/bin:$HOME/.cargo/bin" TABLEPRO_GTK_SCENARIO=clickhouse_structure_distinguishes_unavailable_foreign_keys bash scripts/test-gtk-clickhouse.sh
```

- PostgreSQL scenario: passed in 5.129 seconds; [result](postgres_structure_preserves_empty_and_populated_metadata-result.json).
- ClickHouse scenario: passed in 2.868 seconds; [result](clickhouse_structure_distinguishes_unavailable_foreign_keys-result.json).
- Final preflight: passed, including 579 harness tests, 2,203 unit tests, 291 sandbox tests and the Redis cancellation test.
- Final GTK widget tier: all 176 registered tests passed. Strict app Clippy, docs links, known-issue ledger and ignored-test inventory also passed.
- Historical evidence-validator warnings remained visible; they were not rewritten as current evidence. Debian package validation skipped because `dpkg-deb` is unavailable on this host.

The scenarios live in [gtk_structure.py](../../../apps/linux/app/tests/gtk_structure.py). The PostgreSQL script runs in the existing Forgejo e2e job; the ClickHouse script is part of the local release layer. Both scripts include the new scenario by default and allow focused selection. Each fixture stopped its own application and removed its disposable container. No existing manual-lab database was changed.

## Remaining acceptance

GitHub checks must finish on the final pushed head. Independent review, the maintainer-owned Forgejo gate, native Wayland and system-package acceptance remain separate. The capability umbrella stays open for its remaining consumer decisions and B4 transport handoff.
