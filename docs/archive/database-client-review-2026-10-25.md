# Database-client review, October 25, 2026

## Scope and evidence

This review refreshes the 14-peer inventory against BookiE `636011dd8aa38b6b7ba9a17c54594c9d5247f903`: 13 GitHub repositories and TablePlus official Mac and Linux changelogs. The [manifest](../evidence/client-review-2026-10-25/manifest.json) records all source and release pins. Feature-level comparison covers TablePro, dbx, DBeaver, Beekeeper Studio and dbflux. The other eight GitHub peers and TablePlus receive a source/release freshness check only. The [October 23 inventory](../evidence/client-review-2026-10-23/manifest.json) remains a historical snapshot.

Official repository APIs and release notes supplied default-branch heads, release tags and feature descriptions. Selected upstream changes and the matching BookiE paths were inspected. No third-party application, database, package or benchmark was run. This is not an exhaustive audit of any repository. Source presence is not runtime acceptance, and upstream defects do not establish the same defect in BookiE.

## Source and release pins

| Project | Source pin reviewed | Release observed |
|---|---|---|
| TablePro | [`main` `7dad9d4b260a9bc7389f7778aa0bacb155848906`](https://github.com/TableProApp/TablePro/commit/7dad9d4b260a9bc7389f7778aa0bacb155848906) | Stable [`v0.82.2`](https://github.com/TableProApp/TablePro/releases/tag/v0.82.2); beta [`v0.83.0-beta.2`](https://github.com/TableProApp/TablePro/releases/tag/v0.83.0-beta.2) |
| dbx | [`main` `febdfe08068803f9ddf3660756722473166e6925`](https://github.com/t8y2/dbx/commit/febdfe08068803f9ddf3660756722473166e6925) | Stable [`v0.6.55`](https://github.com/t8y2/dbx/releases/tag/v0.6.55), published October 25 |
| DBeaver | [`devel` `811053f81f8304856ba9091b531bec4eddbf03c6`](https://github.com/dbeaver/dbeaver/commit/811053f81f8304856ba9091b531bec4eddbf03c6) | Community [`26.2.3`](https://dbeaver.io/2026/10/18/dbeaver-26-2-3/), published October 18 |
| Beekeeper Studio | [`master` `5f18a9ee676b808ea243d9a5340fad4b4dd98489`](https://github.com/beekeeper-studio/beekeeper-studio/commit/5f18a9ee676b808ea243d9a5340fad4b4dd98489) | Stable `6.1.6`; beta [`6.2.0-beta.3`](https://github.com/beekeeper-studio/beekeeper-studio/releases/tag/v6.2.0-beta.3) |
| dbflux | [`main` `7cc2261ee2b388b114b66fb793f6aab04d5bd29e`](https://github.com/0xErwin1/dbflux/commit/7cc2261ee2b388b114b66fb793f6aab04d5bd29e) | Stable [`v0.8.13`](https://github.com/0xErwin1/dbflux/releases/tag/v0.8.13), published October 24 |

The remaining peers are included for freshness, not feature parity claims:

| Project | Source and latest stable release observed |
|---|---|
| dbui | `main` `6b98db2e`; `v0.5.1` (October 21) |
| dbdelve | `main` `d1c4ac6b`; `v0.5.1` (October 22) |
| pgui | `main` `d01aed78`; `v0.1.23` (May 9) |
| NavOp | `main` `9df4ebcc`; `v0.21.1` (October 22) |
| fluxDB | `main` `076c27ac`; `v0.1.12` (October 24) |
| Based | `main` `1d4c77da`; `v2026.10.1` (October 24) |
| pgAdmin | `master` `ea293b33`; official stable docs `9.18` (September 17), no GitHub release object |
| DB Browser for SQLite | `master` `ba3a34f7`; stable `3.13.1` (2024), rolling `continuous` build (October 25) |
| TablePlus | No public source pin; official [Mac changelog](https://tableplus.com/blog/2017/02/changelogs.html) lists `27.1.0` (October 20), and [Linux changelog](https://tableplus.com/blog/2020/01/changelogs-linux.html) lists `1.7.0` (August 22) |

Exact source SHAs, release dates and canonical URLs for these peers are in the [manifest](../evidence/client-review-2026-10-25/manifest.json). TablePlus is a different product from TablePro.

TablePro's stable and beta tags predate several October 23–25 Linux-main commits. The new Linux capabilities below are therefore attributed to the reviewed `main` source, not to `v0.82.2`. DBeaver's development branch also contains changes after Community `26.2.3`. Do not treat those source changes as shipped in the listed release.

## Current comparison

### TablePro

Recent Linux-main changes add schema ER diagrams ([#3817](https://github.com/TableProApp/TablePro/pull/3817)), clipboard paste in table tabs ([#3815](https://github.com/TableProApp/TablePro/pull/3815)), SQL editor snippets ([#3819](https://github.com/TableProApp/TablePro/pull/3819)), Cassandra and ScyllaDB ([#3818](https://github.com/TableProApp/TablePro/pull/3818)), DuckDB ([#3806](https://github.com/TableProApp/TablePro/pull/3806)), MongoDB ([#3820](https://github.com/TableProApp/TablePro/pull/3820)) and an AI assistant ([#3822](https://github.com/TableProApp/TablePro/pull/3822)). Its sidebar work also measures 50,000-table catalog cases ([#3830](https://github.com/TableProApp/TablePro/pull/3830), [#3831](https://github.com/TableProApp/TablePro/pull/3831)).

BookiE already has DuckDB and MongoDB static drivers, so those additions close a previously reported peer gap rather than identify missing BookiE work. BookiE has no Cassandra/ScyllaDB drivers, schema diagram or SQL snippet integration in the inspected workspace. Its existing grid paste writes into loaded cells; `paste_plan` refuses to extend beyond the loaded row count. It does not create draft rows. The prior source-pinned Bookie test at `646fe4b5` measured 0.07 seconds in a debug test harness after optimization, against 8.56 seconds for its own baseline. That run includes synthetic tree construction and filtering, excludes GTK and database loading, does not prove current-head performance and is not directly comparable to TablePro's workload or timings ([BookiE evidence](../evidence/postgres-sidebar-scale-2026-10-24/README.md)).

TablePro `v0.82.2` also refuses grid edits and deletes on engines or tables where row identity is ambiguous. That is consistent with BookiE's requirement that writes have usable key identity; it does not show that the implementations or supported engines are equivalent.

### dbx

dbx `v0.6.55` adds, among other work, trigger and native job management for selected engines ([#13403](https://github.com/t8y2/dbx/pull/13403)), schema comparison that can ignore schema-name differences ([#13421](https://github.com/t8y2/dbx/pull/13421)), filtering SQL results by cell value ([#13419](https://github.com/t8y2/dbx/pull/13419)), queued manual exports ([#13441](https://github.com/t8y2/dbx/pull/13441)), generated values during import ([#13408](https://github.com/t8y2/dbx/pull/13408)), Navicat backup import ([#13427](https://github.com/t8y2/dbx/pull/13427)) and RedisDesktopManager profile import ([#13424](https://github.com/t8y2/dbx/pull/13424)).

Bookie currently browses trigger metadata but has no trigger-state mutation action. No table/database comparison flow was identified in the current Bookie app. Bookie already has **Filter by This Value** for browse-grid cells; the current UI review did not establish the same operation over SQL result sets. The import and comparison features remain candidate scope, not selected work. dbx's larger advertised engine count and plugin ecosystem are a different product/architecture choice from Bookie's statically linked workspace drivers.

### DBeaver

DBeaver Community `26.2.3` adds running SQL script files without opening them in the editor and a notification to refresh affected metadata after successful DDL. Bookie opens SQL files as editor tabs and runs them through the existing editor path. Its catalog-change service already refreshes metadata after successful DDL; this part is parity in user outcome, although the interaction differs. DBeaver also has a broader JDBC driver and data-transfer ecosystem. Bookie's current workspace contains eight statically linked engine drivers and does not host runtime driver plugins.

DBeaver's release also records a security fix requiring confirmation before shell commands used to retrieve connection passwords. Bookie has no shell-command password retrieval setting, so this is not a matching Bookie attack path.

### Beekeeper Studio and dbflux

Beekeeper Studio's latest observed stable release remains `6.1.6`; `6.2.0-beta.3` is a pre-release and was published on October 23. No later source head was observed in this refresh. The beta should not be reported as a stable delivery.

dbflux `0.8.13` includes TLS validation, access-parameter validation, grid consistency and older MySQL/MariaDB metadata compatibility fixes. These provide useful test prompts. This source review did not reproduce a corresponding Bookie failure, so it creates no Bookie defect claim.

## Product boundary and follow-up

Bookie is a Linux-native GTK client with static drivers. GUI, MCP and `bookie-agentd` database operations use the same PolicyGuard path; MCP scopes and connection allowlists add separate limits. Typed-value preservation and explicit failure/unknown outcomes remain governing contracts. The product runs without an account, hosted service or paid entitlement ([PRODUCT.md](../../PRODUCT.md), [ADR 0016](../decisions/0016-governed-consumer-seam.md), [ADR 0007](../decisions/0007-type-and-value-preservation.md)).

The comparison therefore leaves these items as unselected candidates, subject to an issue, owner, acceptance and product fit:

- schema diagrams, schema/data comparison and broader migration workflows;
- editor snippets, SQL-file execution without opening an editor tab, and paste-to-new-row behavior;
- third-party connection or data-backup import formats;
- additional database engines such as Cassandra and ScyllaDB.

Do not create issues from this list alone. The active [feature candidate inventory](../0.2-feature-comparison.md) records current scope; the [backlog](../backlog.md) and Project #3 own selected work. A peer's implementation can inform a reproduction or regression test, but is not a reason to weaken Bookie's policy, type, identity, cancellation or resource contracts. No source was copied, no dependency was added and no issue status was changed by this review.
