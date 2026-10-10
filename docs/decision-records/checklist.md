# Decision checklist bridge

Walk of the [standard decision checklist](https://repositorystandards.com/docs/checklist.html) against BookiE on 2026-10-11. Silence is not an answer. Each applicable fork is a link, N/A, or an alignment-backlog item. No new decisions were invented here.

## Foundation

| Fork | Status | Where it lives |
|---|---|---|
| Repo topology | Decided | Cargo workspace under `linux/`. [ARCHITECTURE](../../linux/ARCHITECTURE.md), [AGENTS](../../AGENTS.md). |
| Domain / module boundaries | Decided | Crates sliced by capability (`core`, `policy`, `mcp`, `transport`, `ssh`, drivers). [ARCHITECTURE](../../linux/ARCHITECTURE.md). Static drivers: [ADR 0001](../../linux/docs/decisions/0001-no-plugin-system.md). |
| Two authored descriptions of one structure | Decided | One Cargo graph. No second hand-written build graph for the workspace. Packaging Meson/Flatpak consume the same crates. |
| Language & type strictness | Decided | Rust 1.98, Clippy denies `unwrap` / `expect` / `panic` on production paths. [AGENTS](../../AGENTS.md). |
| Working language | Decided | English in AGENTS writing style and this tree. |
| Dependency & supply-chain policy | Decided | New deps need license and advisory review through `linux/deny.toml`. [AGENTS](../../AGENTS.md). |

## Runtime & data

| Fork | Status | Where it lives |
|---|---|---|
| Datastore & persistence model | Decided | The product is a client, not a hosted store. App state is XDG files plus Secret Service. [ADR 0004](../../linux/docs/decisions/0004-libsecret-secret-storage.md), [ADR 0009](../../linux/docs/decisions/0009-persistence-and-identity-compatibility.md). |
| Schema evolution & migrations | Decided | Compatible adapters and recoverable migrations for saved state. [ADR 0009](../../linux/docs/decisions/0009-persistence-and-identity-compatibility.md). |
| Async, eventing & background jobs | Decided | Relm4 messages and component-scoped commands. No product job queue. [ADR 0003](../../linux/docs/decisions/0003-relm4-architecture.md). |
| Caching | N/A | No application cache layer. Query results are bounded pages, not a cache. |
| Numerical / semantic compatibility | Decided | Native type and value preservation. [ADR 0007](../../linux/docs/decisions/0007-type-and-value-preservation.md). |

## Interfaces & contracts

| Fork | Status | Where it lives |
|---|---|---|
| API / contract style & versioning | Decided | No public HTTP product API. MCP tools plus `tablepro` protocol contracts stay on the rename boundary. [ARCHITECTURE](../../linux/ARCHITECTURE.md). |
| Auth & authorization model | Decided | Desktop: Secret Service. MCP: token scopes, connection allowlists, then `PolicyGuard`. [ADR 0004](../../linux/docs/decisions/0004-libsecret-secret-storage.md), [ADR 0008](../../linux/docs/decisions/0008-connection-and-session-ownership.md), [AGENTS](../../AGENTS.md). |
| Error & result modeling | Decided | Typed `thiserror` at crate boundaries. `DriverError::PolicyDenied` for policy. Panic containment: [ADR 0006](../../linux/docs/decisions/0006-driver-panic-containment.md). |
| Config & secrets management | Decided | Secrets in Secret Service. No secrets in JSON, argv, traces, errors or audit fields. [ADR 0004](../../linux/docs/decisions/0004-libsecret-secret-storage.md), [AGENTS](../../AGENTS.md). |

## Quality & safety

| Fork | Status | Where it lives |
|---|---|---|
| Testing strategy | Decided | Eight tiers, one script and one gate each. [AGENTS](../../AGENTS.md), [validation playbook](../../linux/docs/validation-playbook.md). |
| Observability | Decided | `tracing` fields. Workspace lints deny `print!`. No SQL parameters or secrets in logs. [AGENTS](../../AGENTS.md). |
| Security baseline | Partial | In-code invariants in AGENTS. Dedicated R19-axis ADR is [A-11](../alignment-backlog.md). |
| Accessibility baseline | Undecided | GTK / libadwaita. No WCAG claim. [A-13](../alignment-backlog.md). |
| UX review lens | Undecided | Folded into A-13 until a separate lens is worth a record. |
| Design tokens & design-system handoff | N/A | Platform Adwaita theme. No DTCG token pipeline. |
| Performance & scaling budget | Partial | Query caps, row budgets and PERF-* rows live on the ledger and boards, not a single budget ADR. |

## Delivery

| Fork | Status | Where it lives |
|---|---|---|
| Branching & release strategy | Decided | Work on `linux`. Maintainer cuts releases. Changelog under `[Unreleased]` in `linux/CHANGELOG.md`. [AGENTS](../../AGENTS.md). |
| Integration method & history shape | Undecided | [A-16](../alignment-backlog.md). |
| CI/CD & environments | Decided | GitHub cheap / path-filtered; Forgejo merge tiers. [AGENTS](../../AGENTS.md), [validation playbook](../../linux/docs/validation-playbook.md#ci-tiers). |
| Feature-flagging & rollout | Decided | No account, license or remote entitlement flags. [AGENTS](../../AGENTS.md), [PRODUCT.md](../../PRODUCT.md). |
| Changelog & release notes | Decided | Keep a Changelog 1.1.0 in `linux/CHANGELOG.md`. [AGENTS](../../AGENTS.md). |
| Externally-owned release gate | Undecided | Flathub and distro review exist as packaging work (PKG-4) but the fork is unrecorded. [A-18](../alignment-backlog.md). |

## Product & business

| Fork | Status | Where it lives |
|---|---|---|
| Target personas | Draft | [docs/personas.md](../personas.md). Review: [A-2](../alignment-backlog.md). |
| Pricing / monetization | Decided in PRODUCT | Free, no account. Promote to a BDR under [A-1](../alignment-backlog.md) / [A-14](../alignment-backlog.md) if the wording needs to bind separately. |
| Data retention & compliance | Undecided | [A-12](../alignment-backlog.md). |
| SLAs & support | N/A | Desktop client. No uptime SLA. |
| Vendor / platform lock-in | Decided | GTK / libadwaita and Secret Service are accepted costs. [ADR 0002](../../linux/docs/decisions/0002-rust-gtk4-libadwaita.md), [ADR 0004](../../linux/docs/decisions/0004-libsecret-secret-storage.md). |
| Positioning & messaging | Undecided | [A-14](../alignment-backlog.md). |
| North Star & KPI tree | Draft | [PRODUCT.md](../../PRODUCT.md). Review: [A-1](../alignment-backlog.md). |
| Analytics tracking plan | N/A | No product telemetry pipeline. |
| GTM / launch process | N/A | Scale-profile item. Not used at `core`. |
| Sales / support enablement | N/A | Scale-profile item. Not used at `core`. |
| Legal & compliance surface | Partial | Root [LICENSE](../../LICENSE). Privacy/ToS pages are not a product surface. |
| Open-core / dual-license boundary | N/A | One license for the tree. No `enterprise/` split. |
| Multi-tenant SaaS auth | N/A | Not a hosted multi-tenant service. |
| Node API versioning | N/A | No Node application stack. Layer 2 declined. |
| Flathub as primary distribution | N/A | Packages are GitHub releases today. Flathub is a later packaging row, not the primary channel. |
