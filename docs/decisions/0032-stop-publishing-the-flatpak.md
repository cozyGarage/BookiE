# 0032: Stop publishing the Flatpak

- **Status**: Accepted (2026-11-04)
- **Date**: 2026-11-04

## Context

[ADR 0031](0031-stop-publishing-the-debian-package.md) left three package jobs: Arch, AppImage and Flatpak. The maintainer will support the GTK application on Arch Linux and as an AppImage only for now. Other platforms are covered by the isolated Tauri and GPUI experiments ([ADR 0028](0028-isolated-frontend-experiments.md)), which are not releases.

## Decision

The release workflow stops building and publishing the Flatpak bundle, and the release recovery check no longer requires it. The Arch package and the AppImage must both succeed before a release is drafted.

The Flatpak manifests, `scripts/build-flatpak.sh`, the packaging-only pull request workflow and the sandbox handling in the application stay in the tree, so the Flatpak can return without rework.

## Rationale

Every published package is a support promise. The Flatpak carries a second runtime (GNOME 51), a sandbox that changes SSH, keyring and file access, and its own failure modes, for a user base the maintainer cannot test today.

## Alternatives considered

- Keep publishing the Flatpak and label it unsupported. Rejected: a published artifact reads as supported.
- Delete the manifests and sandbox code now. Rejected: it is a larger change that is costly to undo; stopping publication is enough for 0.2.5.

## Consequences

- Release 0.2.4 keeps its Flatpak. Later releases do not carry one.
- ADR 0031 still says the Flatpak is the install path on Debian-family systems; that no longer holds from 0.2.5. The AppImage is the only option there.
- Removing the packaging-only Flatpak workflow, the manifests and the sandbox code is a later change, if the Flatpak is not coming back.
