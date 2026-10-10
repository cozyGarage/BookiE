# Adoption assessment - BookiE

Gate 2 artifact. What the eight-pass read found on 2026-10-10, before Wave 1 changed the tree. Updated in place. The count a human says go or no-go on is in [`docs/backlog.md`](backlog.md).

Baseline mechanical score, same day: `node .repository-standards/standard/scripts/self-verify.mjs --warn --profile core` reported **drift 4** against the built-in skeleton (no manifest yet). That is not the distance from the full standard. After waves 1–4, `node scripts/self-verify.mjs --warn --profile core` is the living number; refresh the cache with the degit command in [`standards-updates.md`](standards-updates.md) when you need the full standard tree again.

## Maturity per pass

| # | Pass | Maturity | What that rests on |
| --- | --- | --- | --- |
| 1 | Skeleton & docs | partial | `AGENTS.md`, `CLAUDE.md`, root `README.md`, `linux/ARCHITECTURE.md`, `linux/docs/README.md`, living changelog and sprint exist. No `PRODUCT.md`, personas, `PRINCIPLES.md`, public `SECURITY.md`, `.standards-version`, or `standard.manifest.json`. |
| 2 | Decisions in code | partial | Fifteen ADRs in `linux/docs/decisions/` cover drivers, GTK, Relm4, Secret Service, cancellation, panic, type/value, session ownership, persistence, audit, paging, sidebar, result sets, export snapshot, and snapshots. The standard checklist had not been walked; several product forks (retention, accessibility, git history shape) are silent. |
| 3 | Capabilities & specs | absent | Behaviour lives in AGENTS, ARCHITECTURE and ADRs. No `specs/<capability>/spec.md` and no `specs/capability-map.json`. |
| 4 | Quality gates | solid | Eight named test tiers, each with one script and one gate. `preflight.sh` runs file-size, function-size, panic-site, bounded-operation, fmt, Clippy (`-D warnings`), unit and sandbox. Driver, TLS, release, GTK and keyring tiers exist. |
| 5 | CI/CD | solid | Forgejo `forgejo-gate` is the merge acceptance path. GitHub runs cheaper / path-filtered jobs. Those gates fire on real PRs; a workflow file existing is not the evidence. |
| 6 | Security & supply chain | partial | `PolicyGuard` on every consumer handle; MCP scopes and allowlists; Secret Service via `tablepro-storage`; `cargo deny` and `linux/deny.toml`. No public vulnerability-contact file. No gitleaks workflow. Private advisory link already sits in `.github/ISSUE_TEMPLATE/config.yml`. |
| 7 | Dependencies & stack | solid | Rust/GTK workspace under `linux/`. Drivers are static crates. No web view, no cross-platform UI layer. Layer 2 Node stack does not apply and is declined. |
| 8 | Drift & health | partial | Docs have an ownership map and recent trim of duplicated README/ARCHITECTURE facts. Known-issues ledger is the open-work source. No self-verify bookmark, so standard-shaped drift was unmeasured. |

## Top risks

1. Copying BookiE rules into a second AGENTS or moving `linux/docs/decisions/` would split authority and break scripts that depend on those paths. The standard merely made the dual-home risk visible.
2. Turning drift 0 into a Forgejo merge requirement before the tree is ready would block product work on a number that is not yet the product's truth.
3. A hollow `specs/` tree (headings without contracts) would raise the adopted percentage without making PolicyGuard or SSH rebuildable.
4. Relocating the changelog, ledger or Cargo workspace to match the standard's paved paths would fight the Linux-only workspace layout AGENTS already owns.

## Findings by owner role

### product

| Finding | Where it goes |
| --- | --- |
| No product vision page or persona roster the spec gate can hold | A-1, A-2 |
| Pricing is decided (free, no account) but not written as a BDR | A-1, A-14 |
| Data retention, accessibility and positioning are silent | A-12, A-13, A-14 |

### architect

| Finding | Where it goes |
| --- | --- |
| Decision checklist not walked against existing ADRs | Wave 2 index; leftovers A-8, A-11, A-16, A-18 |
| No capability specs or coupling map for PolicyGuard / SSH | Wave 3; further slices A-4, A-5, A-6 |
| Profile `core` vs later `scale` is intentional and unrecorded as a decision | A-8 |
| Security baseline axes are in AGENTS, not a dedicated ADR | A-11 |
| Rename-boundary and CI-tier facts are restated in more than one living doc | A-7 |

### dev

| Finding | Where it goes |
| --- | --- |
| No vendored `self-verify` or advisory drift report | Wave 4; optional gitleaks A-17 |

### agent

| Finding | Where it goes |
| --- | --- |
| Missing standard bookmark files and a decision-records index that points at `linux/docs/decisions/` | Wave 1 |
| Claude lifecycle skills and R22 loop headings do not match this Cursor/AGENTS repo | A-15 |

## Non-goals for wave 1

These stay out of the first structural change, even after this assessment:

- Relocating ADRs out of `linux/docs/decisions/`
- Changing Forgejo workflows or `forgejo-gate` semantics
- Changing `linux/Cargo.toml` or driver crates
- Enabling a blocking GitHub `spec-guard` job
- Adopting a Layer 2 Node stack
- Mixing uncommitted README/ARCHITECTURE trim into the standards change unless the owner combines them on purpose

## Closed by the alignment wave itself

Closed while implementing the locked gradual plan on 2026-10-11. They are not outstanding in the Gate 5 count.

- Cached `repository-standards/core` 1.0.19 under gitignored `.repository-standards/` and recorded the skeleton baseline (drift 4).
- Wrote this assessment, the intake record, and the counted alignment backlog.
- Added PRODUCT, personas, PRINCIPLES, SECURITY, the decision-records index, SPEC/manifest/version bookmark, and a thin AGENTS/CLAUDE bridge. No Rust or Forgejo change.
- Walked the decision checklist against existing ADRs and marked N/A or queued leftovers.
- Extracted PolicyGuard and SSH capability specs and bound them in `specs/capability-map.json`.
- Vendored `scripts/self-verify.mjs` and helpers; added an advisory GitHub drift workflow and `docs/standards-updates.md`.
- Living mechanical score after those waves: `node scripts/self-verify.mjs --warn --profile core` reports **drift 0** (90% of the core manifest present, 6 recorded exceptions). Three `[NEEDS REVIEW]` markers remain (A-1, A-2, A-3). Drift 0 is structural, not a claim that every later-wave item is done.
