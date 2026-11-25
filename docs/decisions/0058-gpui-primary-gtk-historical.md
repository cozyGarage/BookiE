# 0058: GPUI is the primary frontend; GTK is historical

- **Status**: Accepted (2026-11-26)
- **Date**: 2026-11-26
- **Supersedes**: ADR 0052

## Context

GPUI has become the main frontend direction, but ADR 0052 still requires GTK to be maintained in parallel. That wording leaves feature work, release acceptance, issue triage and package ownership ambiguous. GTK is the existing Linux application and remains the released product until GPUI clears a measured release gate; compile and headless CI alone do not establish that gate.

## Decision

1. **Frontend direction.** GPUI is BookiE's primary frontend for new product work. New UI features, design changes and frontend-specific architecture work target `apps/gpui`. Shared behavior belongs in the existing UI-free crates and is not duplicated for either frontend.
2. **GTK historical maintenance.** The GTK application is a historical Linux frontend. It receives security fixes, data-integrity fixes, and fixes for regressions that prevent an existing supported release from building, launching, connecting or safely operating. It receives no new user-facing features, parity work or broad UI refactors. Existing GTK release artifacts remain available. GTK-specific backlog issues use the `gtk-historical` label; shared driver, policy and app-services issues do not receive it solely because GTK also consumes that code.
3. **GPUI supported-release gate.** A GPUI preview is not a supported release. Linux and macOS are the initial supported-platform targets; each must pass the gate independently. Adding another platform requires the same package, accessibility and installed-acceptance evidence before it is named as supported. Before GPUI becomes the primary supported artifact, the release candidate must:
   - pass the required Linux and macOS CI checks for the exact source revision;
   - exercise security invariants through app-services, including policy denial, approval, audit terminal outcomes, typed values and credential boundaries;
   - pass keyboard and screen-reader accessibility checks on each supported platform, with the findings and environment recorded;
   - provide a packaged artifact for each supported platform and pass installed launch, connection, query, browse, safe-write, export and update or removal checks applicable to that platform;
   - complete the release-candidate manual acceptance issue with no unresolved release-blocking correctness, security, data-loss or accessibility defects.

   Compile tests, headless tests, screenshots of an uninstalled build, and preview use do not substitute for installed acceptance.
4. **Issue tracking.** `gtk-historical` marks GTK-only maintenance work and excludes feature parity from the issue's acceptance. Issues that affect shared crates or both frontends remain ordinary product issues and follow their normal acceptance criteria. The current manual acceptance checklist separates the remaining GTK release checks from the GPUI supported-release gate.
5. **Release artifacts and cutover.** Until the GPUI gate passes, GTK remains the primary released Linux artifact and GPUI assets, if published, are clearly labeled unsupported previews. The first supported GPUI release makes GPUI the primary download and default installation path for every platform it supports. That release may include the last GTK Linux package as an optional asset labeled “Legacy GTK”; subsequent releases do not build or publish GTK packages unless the maintainer reopens this decision. Existing GTK releases stay available for rollback and existing users. If a GPUI release candidate fails the gate, keep the GTK Linux artifact primary and do not announce the cutover.
6. **Shared architecture.** Both frontends continue to use app-services, results, the driver registry, PolicyGuard and the shared typed command/event contracts. This decision does not authorize moving domain or policy behavior into either UI or relaxing any accepted security, value or audit contract.

## Rationale

One primary frontend gives new UI work a clear home and avoids spending each feature twice. Keeping the last GTK release available protects Linux users while GPUI earns support through installed accessibility, packaging and data-path acceptance. The gate lets the project switch defaults based on reproducible evidence rather than the frontend's implementation status.

## Alternatives considered

- **Continue maintaining both frontends equally:** rejected because it doubles UI feature and acceptance work while leaving ownership unclear.
- **Switch release artifacts immediately:** rejected because GPUI does not yet meet the installed accessibility and packaging gate.
- **Delete GTK and its release artifacts now:** rejected because users still depend on the supported Linux package and GPUI is not yet a supported replacement.

## Consequences

- New UI issues target GPUI unless they explicitly concern GTK historical maintenance.
- The `gtk-historical` label signals limited maintenance scope; it does not waive fixes needed to protect user data or security.
- GTK package jobs remain required only while the GTK artifact is still the primary Linux release or while preparing the final legacy package.
- The maintainer must approve and record the exact GPUI supported-platform matrix in the release candidate checklist before artifact cutover.
- ADR 0052 remains as the historical record of the initial parallel-maintenance decision and is superseded by this ADR.
