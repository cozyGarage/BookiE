# 0031: Stop publishing the Debian package

- **Status**: Accepted (2026-11-04)
- **Date**: 2026-11-04

## Context

[ADR 0021](0021-toolkit-floor.md) sets the toolkit floor at GTK 4.24, libadwaita 1.10, GtkSourceView 5.22 and GLib 2.88. No current Ubuntu or Debian stable release ships those versions, so the Debian package built against Debian testing cannot be installed on the releases most Debian-family users run. ADR 0021 listed native Arch and Debian packages as the primary 0.2 targets.

## Decision

The release workflow stops building and publishing the Debian package, and the release recovery check no longer requires it. The three remaining package jobs, Arch, AppImage and Flatpak, must all succeed before a release is drafted.

The Debian builder and its validator stay in the repository, because the AppImage build stages its files with them and the distro-floor tests build with them. The Debian testing image used to build and test the application is unchanged.

## Rationale

A published package that cannot be installed on the systems it appears to target gives users a failing install. The AppImage and Flatpak carry their own toolkit and run on those systems.

## Alternatives considered

- Keep publishing the package and document that it needs Debian testing. Rejected: users on Debian and Ubuntu stable would still find it and fail to install it.
- Lower the toolkit floor so the package installs on stable releases. Rejected: ADR 0021 sets the floor for the application itself.

## Consequences

- Release 0.2.4 keeps its Debian package. Later releases do not carry one.
- ADR 0021 still describes the floor; its mention of Debian as a primary native target no longer holds for releases from 0.2.5.
- Users on Debian-family systems install the AppImage or the Flatpak.
