# 0021: One GNOME 50 native library baseline

- **Status**: Accepted
- **Date**: 2026-10-23
- **Supersedes**: ADR 0002's distinction between the Cargo floor and package line

## Context

Maintaining an older Cargo stack alongside a newer package stack duplicates compatibility work and runner qualification. The maintainer selected Ubuntu 26.04 LTS and current native libraries for subsequent builds.

## Decision

Cargo, Meson, package dependencies and GTK CI use GTK 4.22, GLib 2.88, libadwaita 1.9 and GtkSourceView 5.18. Ubuntu 26.04 is the distro-floor target. Stock Ubuntu 24.04 and Debian 13 libraries no longer qualify new builds. ADR 0002's Rust/native Linux architecture remains in force; Rust remains pinned to 1.98.

## Consequences

Package managers must reject installations with older native libraries. Older hosts can execute headless tests or run GTK through compatible containers. Runner replacement is validated before retiring the old runner. The completion-popup failure remains independently tracked in issue 1445; changing the baseline does not establish its cause or fix it.

Cargo dependency versions and vendored driver patches are not upgraded incidentally. Regenerate the offline vendor artifact from the locked inputs when packaging the new build.

## Alternatives considered

Retaining both native floors costs additional qualification and was not selected. Upgrading a runner alone leaves the product's declared requirements inconsistent.
