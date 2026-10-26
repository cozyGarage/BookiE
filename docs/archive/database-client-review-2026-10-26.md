# Database-client review, October 26, 2026

## Scope and evidence

This refresh checks BookiE against 14 peers: 13 GitHub repositories and TablePlus through its official Mac and Linux changelogs. The [manifest](../evidence/client-review-2026-10-26/manifest.json) pins their current source and release states. Feature-level review covers TablePro, dbx, DBeaver Community, Beekeeper Studio and dbflux. Other repositories receive freshness checks only.

The review uses BookiE feature code at `636011dd8aa38b6b7ba9a17c54594c9d5247f903`. GitHub `linux` is now `e19caf69162917a1e074d0fae83017e96f146be7`, sixteen commits ahead. Seven application or GTK-test files changed after the feature-code pin. The application changes in #1666 replace reads of individual driver declarations with `DatabaseDriver::capabilities()`; the PR says the driver methods remain the source of truth, and the reviewed replacements do not change the feature comparisons below. PR #1665 extends streamed-pager test coverage. The remaining changes are docs and evidence. The local checkout remains at the older feature-code commit.

Official GitHub APIs, release notes, selected merged PRs and current BookiE code supplied the findings below. This is not an exhaustive audit of any peer. No third-party application, database, package or benchmark was run. Results and timings from upstream PRs are reported as their authors' evidence and are not compared as equivalent workloads.

## Current source and releases

| Project | Current source | Latest stable observed | Relevant change since October 25 |
|---|---|---|---|
| TablePro | [`main` `0f74142e`](https://github.com/TableProApp/TablePro/commit/0f74142ec30c739544d5f00680d4216099569b13) | [`v0.83.0`](https://github.com/TableProApp/TablePro/releases/tag/v0.83.0), October 26 | The release adds Search Database, Compare Results and Generate Rows. Linux main also merges a shared native grid ([#3836](https://github.com/TableProApp/TablePro/pull/3836)) and a large-catalog sidebar performance change ([#3842](https://github.com/TableProApp/TablePro/pull/3842)). |
| dbx | [`main` `6eac6d93`](https://github.com/t8y2/dbx/commit/6eac6d939ac6eb01dfdb01ada740b58d31118827) | [`v0.6.55`](https://github.com/t8y2/dbx/releases/tag/v0.6.55), October 25 | Main advanced after the stable release. One new main-only feature adds ordered local SQL file includes ([commit](https://github.com/t8y2/dbx/commit/b5b3c771b2eb18a56ce385156915885ff4caa6ed)). |
| DBeaver Community | [`devel` `a79062e5`](https://github.com/dbeaver/dbeaver/commit/a79062e59e2f3c3b128acfa6e4e1b5ef0dd7f262) | [`26.2.3`](https://github.com/dbeaver/dbeaver/releases/tag/26.2.3), October 18 | Development source merges result-grid cell drag and drop into the SQL editor as literals ([#42526](https://github.com/dbeaver/dbeaver/pull/42526)); the stable release does not yet include it. |
| Beekeeper Studio | [`master` `84df8c7c`](https://github.com/beekeeper-studio/beekeeper-studio/commit/84df8c7c6da79612d6084a99a0ce9af0857f79b2) | `6.1.6`; beta [`6.2.0-beta.3`](https://github.com/beekeeper-studio/beekeeper-studio/releases/tag/v6.2.0-beta.3) | The table-filter builder and Quick Filter gained a `not in` operator ([#4903](https://github.com/beekeeper-studio/beekeeper-studio/pull/4903)). |
| dbflux | [`main` `9ed2818d`](https://github.com/0xErwin1/dbflux/commit/9ed2818d485fdc95d9a6e82116df08b5798f9cae) | [`v0.8.13`](https://github.com/0xErwin1/dbflux/releases/tag/v0.8.13), October 24 | Its latest main commit adds a custom AWS endpoint for Secrets Manager and SSM value sources; no corresponding BookiE defect was identified. |

TablePro and DBeaver each have source changes newer than their listed stable releases. dbx's new SQL include work is on main after `v0.6.55`. These are source observations, not stable-release claims.

The eight freshness-only GitHub peers are dbui `0.5.1`, dbdelve `0.5.1`, pgui `0.1.23`, NavOp `0.21.2`, fluxDB `0.1.12`, Based `2026.10.1`, pgAdmin `9.18`, and DB Browser for SQLite `3.13.1` stable with a newer nightly build. TablePlus is separate from TablePro; its latest listed releases are Mac `27.1.0` (October 20) and Linux `1.7.0` (August 22). Full pins and dates are in the [manifest](../evidence/client-review-2026-10-26/manifest.json).

## Feature comparison

### TablePro

TablePro `v0.83.0` now includes three features relevant to BookiE's candidate inventory:

- **Search Database** finds values across tables and columns, then opens a matching table filtered to its rows ([#3737](https://github.com/TableProApp/TablePro/pull/3737)). No cross-table value search was identified in BookiE's current app.
- **Compare Results** compares two query results by key columns or whole rows ([#3739](https://github.com/TableProApp/TablePro/pull/3739)). BookiE has no result-set comparison workflow. This differs from comparing schema or database objects.
- **Generate Rows** creates repeatable test data while preserving keys, enums and foreign keys ([#3759](https://github.com/TableProApp/TablePro/pull/3759)). No test-row generator was identified in BookiE.

The current Linux branch also consolidates table, SQL-result and key grids around one native grid and shared rules. Its PR describes stable row slots, a common action group and logic moved into GTK-free services ([#3836](https://github.com/TableProApp/TablePro/pull/3836)). This is an architecture reference, not evidence that the two applications have the same grid behavior or contracts.

The large-catalog PR reports measurements from a Mac with an in-process event driver and explicitly says that its keyboard-driven run lacked Accessibility permission. For 50,000-table fixtures, it reports different outcomes by step and fixture, including remaining stalls above 100 ms ([#3842](https://github.com/TableProApp/TablePro/pull/3842)). BookiE's prior 50,000-table evidence is a synthetic debug test of tree construction and filtering, not a GTK or database-load run ([BookiE evidence](../evidence/postgres-sidebar-scale-2026-10-24/README.md)). The workloads, platforms and measurement boundaries differ, so their timings should not be compared directly.

BookiE already has DuckDB and MongoDB drivers, bounded clipboard updates to loaded cells, typed-value handling and refusal when paste would extend beyond the loaded page. TablePro's broader feature set does not change those existing behaviors.

### dbx

Stable `v0.6.55` includes schema comparison options, selected-engine trigger management, generated import values and queued manual exports. BookiE browses trigger metadata but has no trigger-state mutation action or schema/data comparison workflow. It has **Filter by This Value** in the browse grid; the source review did not establish the same operation on SQL result sets.

The newer dbx main branch adds ordered local SQL file includes. BookiE opens SQL files in editor tabs and has no include directive or file-include runner in the inspected path. This remains an unselected candidate. Any implementation would need bounded file count, size and recursion, safe path handling, and the existing statement planning, PolicyGuard, audit and outcome path. The main-only feature is not part of `v0.6.55`.

### DBeaver Community

Community `26.2.3` can execute a SQL script file without opening it in the editor and offers a refresh notification for affected metadata after successful DDL. BookiE opens SQL files as editor tabs; successful DDL already refreshes its catalog metadata. The refresh outcome is present, while the standalone file-run interaction is not.

The newer DBeaver development change lets users drag one result column into the editor as comma-separated SQL literals, or multiple columns as CSV-like text. BookiE already offers **Copy as IN clause** and **Copy as INSERT** from its grid, but no drag-to-editor action was identified. DBeaver's PR notes that timestamp-with-time-zone values currently lose the zone during single-column drag; any analogous BookiE action would need a dialect-safe typed literal contract. This is a workflow candidate, not a missing copy capability.

### Beekeeper Studio and dbflux

Beekeeper's new `not in` filter is partly covered already: BookiE's filter model and browse filter strip support **is none of** (`FilterOp::NotIn`). The cell context menu has **Filter by This Value**; a one-step **Exclude Value** context action was not identified. This is an interaction difference, not a missing `NOT IN` filter operator.

dbflux's `v0.8.13` continues to provide useful TLS, input-validation and MySQL/MariaDB metadata test prompts. Its newest main commit is about custom AWS value-source endpoints. Neither finding reproduced a matching BookiE defect.

## Follow-up candidates

These are candidates for product review, not selected work or issue assignments:

- cross-table search for a value, with explicit scope, time and row limits;
- comparison of two query result sets, with typed-value and key matching rules;
- repeatable test-row generation that respects generated columns, constraints and policy;
- local SQL-file includes, with bounded and governed execution;
- drag selected result cells into the editor as SQL literals, if it improves on BookiE's current copy actions.

The [active feature comparison](../0.2-feature-comparison.md), backlog and Project #3 determine selection and ownership. Preserve BookiE's value, identity, cancellation and PolicyGuard contracts when considering any peer behavior. No source was copied, no dependency was added and no issue status changed in this review.
