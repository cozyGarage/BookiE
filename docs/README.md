# Documentation map

Rules for contributors are in [AGENTS.md](../AGENTS.md). Accepted ADRs define architecture and behavior; the completion page records release status, while Project #3 and its issues own selected work and acceptance.

## Documents by audience

| Audience | Documents |
|---|---|
| People using BookiE | [User guide](user-guide.md), [feature list](features.md), [platforms](platforms.md), [root README](../README.md) |
| Contributors | [AGENTS.md](../AGENTS.md), [architecture](../ARCHITECTURE.md), [code conventions](code-conventions.md), [adding drivers](adding-drivers.md), [state management](state-management.md), [value contract rules](value-contract-rules.md), [connections](connections.md), [storage](storage.md), [error handling](error-handling.md) |
| Testing | [Validation playbook](validation-playbook.md), [testing](testing.md), [GTK testing](testing-gtk.md), [CI testing](testing-ci.md), [test design rules](test-design-rules.md), [manual verification](manual-verification-0.2-features.md) |
| Planning and status | [0.2.0 completion](bookie-0.2-sprint.md), [work selection](backlog.md), [known limitations](known-limitations.md), [Project #3](https://github.com/users/cozyGarage/projects/3), [issue ledger](known-issues.md), [changelog](../CHANGELOG.md) |
| Product and security | [PRODUCT.md](../PRODUCT.md), [personas](personas.md), [PRINCIPLES.md](../PRINCIPLES.md), [SECURITY.md](../SECURITY.md), [capability specs](../specs/README.md) |
| Decisions | [ADRs](decisions/README.md), [proposals](proposals/README.md) |
| Project history | [0.2.0 development history](archive/bookie-0.2-history.md), [adopted-code attribution](upstream-sync.md) |

## Where each fact belongs

| Fact | Owner |
|---|---|
| Contributor rules | [AGENTS.md](../AGENTS.md) |
| Product constraints and user outcomes | [PRODUCT.md](../PRODUCT.md), [personas](personas.md), [PRINCIPLES.md](../PRINCIPLES.md) |
| Current behavior extracted from code | [specs](../specs/README.md); update a spec when it disagrees with code |
| Architecture or behavior decision | An [ADR](decisions/README.md) |
| Open work and acceptance | [Project #3](https://github.com/users/cozyGarage/projects/3); the [issue ledger](known-issues.md) maps IDs to issues |
| Release completion and scope | [Completion page](bookie-0.2-sprint.md) and Project #3 |
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

## Archive

[archive/bookie-0.2-history.md](archive/bookie-0.2-history.md) records stable release history. Run reports, profiling records, dated research and raw evidence are kept in the private research repository.
