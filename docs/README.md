# Documentation map

Rules for contributors are in [AGENTS.md](../AGENTS.md). Accepted ADRs define architecture and behavior; the public issue tracker owns selected work and acceptance criteria.

## Documents by audience

| Audience | Documents |
|---|---|
| People using BookiE | [User guide](user-guide.md), [feature list](features.md), [platforms](platforms.md), [root README](../README.md) |
| Contributors | [AGENTS.md](../AGENTS.md), [architecture](../ARCHITECTURE.md), [code conventions](code-conventions.md), [adding drivers](adding-drivers.md), [state management](state-management.md), [value contract rules](value-contract-rules.md), [connections](connections.md), [storage](storage.md), [error handling](error-handling.md), [vendor patch guide](vendor-patches.md) |
| Testing | [Validation playbook](validation-playbook.md), [testing](testing.md), [GTK testing](testing-gtk.md), [CI testing](testing-ci.md), [test design rules](test-design-rules.md), [manual verification](manual-verification-0.2-features.md) |
| Current work | [Work selection](backlog.md), [Project #3](https://github.com/users/cozyGarage/projects/3), [issue ledger](known-issues.md), [known limitations](known-limitations.md), [changelog](../CHANGELOG.md) |
| Product and security | [PRODUCT.md](../PRODUCT.md), [personas](personas.md), [PRINCIPLES.md](../PRINCIPLES.md), [SECURITY.md](../SECURITY.md), [capability specs](../specs/README.md) |
| Decisions and history | [ADRs](decisions/README.md), [adopted-code attribution](upstream-sync.md), [release history](../CHANGELOG.md) |

## Where each fact belongs

| Fact | Owner |
|---|---|
| Contributor rules | [AGENTS.md](../AGENTS.md) |
| Product constraints and user outcomes | [PRODUCT.md](../PRODUCT.md), [personas](personas.md), [PRINCIPLES.md](../PRINCIPLES.md) |
| Current behavior extracted from code | [specs](../specs/README.md); update a spec when it disagrees with code |
| Architecture or behavior decision | An [ADR](decisions/README.md) |
| Open work and acceptance criteria | [Project #3](https://github.com/users/cozyGarage/projects/3); the [issue ledger](known-issues.md) maps IDs to issues |
| Build and test commands | [Validation playbook](validation-playbook.md) and [testing](testing.md) |
| User-visible restrictions | [Known limitations](known-limitations.md) |
| User guidance | [User guide](user-guide.md) |
| User-facing change note | `Release-Note:` in the pull request body and squash commit, per [ADR 0022](decisions/0022-release-notes-from-merge-commits.md) |

Run-specific evidence, profiling output, research notes and review records are maintained outside the public source tree. Keep reproducible tests, fixtures, test commands, stable product behavior and architecture decisions here. Do not commit machine-specific paths, credentials, customer data or raw run output.

## Adding documentation

- Update the existing owner document when possible; add a new file only for a distinct audience or concern.
- Put accepted architecture decisions in sequentially numbered ADRs and list them in the ADR index.
- Keep public testing instructions generic and reproducible. Store machine-specific results outside this repository and refer to the owning issue when public acceptance status matters.
- When a document becomes obsolete, remove or replace it and update its links. `scripts/check-doc-links.py` checks local links.
- Keep public file paths stable when scripts or workflows depend on them.
