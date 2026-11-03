# 0028: Isolate frontend experiments from the shipped application

- **Status**: Accepted
- **Date**: 2026-11-03

## Context

The shipped Linux client uses GTK4 and libadwaita under [ADR 0002](0002-rust-gtk4-libadwaita.md). Evaluating a potential future frontend requires runnable evidence for large results, desktop interaction and development cost. An experiment is not evidence of support for another operating system or a decision to replace the shipped UI.

## Decision

Allow isolated frontend experiments in `experiments/`. They may use a web view or a cross-platform UI toolkit to render synthetic data. They are not shipped, included in application packages or connected to user databases, credentials, stored state or network services. The existing Linux application remains governed by ADR 0002 and its security and type contracts.

Each experiment has its own dependency lockfile, license and advisory checks, tests and instructions. It must identify its limits and must not be presented as a supported client. Any proposal to connect an experimental frontend to real data or ship it requires a separate architecture and product decision, including typed values, policy, audit, accessibility and platform acceptance.

## Consequences

The repository can compare frontend options without introducing an alternate production execution path. Experiments add maintenance and dependency review cost. A successful synthetic grid does not prove real workload performance or feature parity.

## Alternatives considered

- Prohibit runnable experiments: this prevents measuring the interaction and development tradeoffs before a product decision.
- Replace the GTK client immediately: this commits to a rewrite before the type, security, accessibility and platform requirements are demonstrated.
