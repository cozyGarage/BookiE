# Evidence directory index (2026-10-10 consolidation pass 4)

Inventory of `linux/docs/evidence/` directories at tip sync for docs
cleanup PR #498. **Do not delete** red, incomplete or orphan
manifests. Live “test again” signals stay on the sprint, B3 and B4
retest tables; this file only classifies directories.

## Counts

| Class | Count | Meaning |
| --- | ---: | --- |
| Historical pass (no open `not_established`) | 345 | Passed run; keep as source-pinned proof |
| Local pass with open bounds | 51 | Passed locally; package / Wayland / Forgejo / frozen-candidate still open in the manifest |
| Passed with retest signal | 4 | Current run passed; manifest notes intermittent / unexplained / infra flake |
| Review notes prior failures | 2 | Manifest records prior failed attempts; not a live red gate by itself |
| Orphan (no `manifest.json`) | 5 | Keep until a maintainer decides; do not invent a manifest |
| **Total directories** | **407** | |

## Live retest tables (authoritative)

- [Sprint retest indicators](../bookie-0.2-sprint.md#retest-indicators-do-not-drop)
- [B3 retest indicators](../type-contract-strategy.md#retest-indicators)
- [B4 retest indicators](../b4-task-board.md#retest-indicators-from-archived-checkpoints)

## Passed with retest signal

| Directory | Note |
| --- | --- |
| [aud2-current-linux-tip-735baf1e-2026-10-10](../evidence/aud2-current-linux-tip-735baf1e-2026-10-10/manifest.json) | manifest notes intermittent/unexplained/infra flake |
| [b4-aud2-gtk-rerun-2026-10-09](../evidence/b4-aud2-gtk-rerun-2026-10-09/manifest.json) | manifest notes intermittent/unexplained/infra flake |
| [b4-aud2-repeat-2026-10-09](../evidence/b4-aud2-repeat-2026-10-09/manifest.json) | manifest notes intermittent/unexplained/infra flake |
| [mssql-datetimeoffset-grid-edit-results-2026-10-04](../evidence/mssql-datetimeoffset-grid-edit-results-2026-10-04/manifest.json) | manifest notes intermittent/unexplained/infra flake |

## Review manifests that record prior failed attempts

| Directory | Note |
| --- | --- |
| [architecture-review-2026-10-03](../evidence/architecture-review-2026-10-03/manifest.json) | records prior failed attempts |
| [b3-review-2026-10-01](../evidence/b3-review-2026-10-01/manifest.json) | records prior failed attempts |

## Orphan directories (no manifest)

| Directory | Note |
| --- | --- |
| `2026-09-29-regression-audit/` | missing manifest.json |
| `2026-09-bug-consistency/` | missing manifest.json |
| `2026-09-stabilization/` | missing manifest.json |
| `sql-lex-mutation-results-2026-10-06/` | missing manifest.json |
| `upstream-u1-2026-10-03/` | missing manifest.json |

## Local-pass directories with open bounds

51 directories include a `not_established` field (typical:
Forgejo gate, frozen candidate, package-installed or native Wayland).
They are not deleted and are not listed individually here; open the
owning ledger or board row for the current bound.

## Rule

Failed-case and incomplete indicators stay until cleared on a current
candidate SHA. A green tip alone does not clear TEST-31, AUD-2 empty-dialog,
MongoDB TLS intermittent, or Hub rate-limit signals.
