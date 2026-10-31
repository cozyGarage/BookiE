# 0021: Support only the newest GTK toolkit

- **Status**: Accepted
- **Date**: 2026-10-23

## Context

[ADR 0002](0002-rust-gtk4-libadwaita.md) set separate compile and package toolkit floors. Maintaining compatibility code for older toolkit lines adds implementation and validation work without changing the product's native UI contracts. This decision sets one supported native-library floor.

## Decision

The maintainer decided on 2026-10-23 that BookiE supports one toolkit floor, the newest stable line:

| Library | Minimum |
|---|---|
| GTK 4 | 4.24 |
| libadwaita | 1.10 |
| GtkSourceView 5 | 5.22 |

GLib stays at 2.88. The Flatpak manifests use the GNOME 51 runtime.

The floor is enforced in three places:

- The application checks the running library versions at startup and exits with a message naming the required and found versions when any is older.
- The root Meson project requires these versions.
- The Debian and Arch packages declare versioned dependencies on them.

The current gtk-rs features match the toolkit floor: GTK compiles at `gnome_50` plus `v4_24`, libadwaita at `v1_10`, GtkSourceView at `v5_22`, and GLib at `v2_88`. The shortcuts window now uses `AdwShortcutsDialog`.

CI validates the selected toolkit floor. Compatibility with older toolkit versions is not tested, and fixes are not backported to them.

## Rationale

One floor removes per-version branches from the app and the test tier and makes the floor the same stack developers run. Users on older distributions have a path that does not depend on BookiE carrying workarounds: a newer distribution release or, once offered, the Flatpak build.

## Consequences

- Ubuntu 26.04 LTS (GTK 4.22.4, GtkSourceView 5.18.0, libadwaita 1.9.1), Debian 13 (GTK 4.18.6, GtkSourceView 5.16.0, libadwaita 1.7.6) and Ubuntu 24.04 (GTK 4.14.5, GtkSourceView 5.12.0, libadwaita 1.5.0) are unsupported. The Debian package does not install there and a source build exits at startup.
- Flatpak on the GNOME 51 runtime is the intended path for those distributions.
- CI and native packages must use the selected toolkit floor.
- When a newer toolkit becomes the stable line on the reference distributions, the floor moves with it by a new decision.

## Alternatives considered

- Keep the two-floor model. It keeps the workaround cost that prompted this decision.
- Use Ubuntu 26.04 LTS (GTK 4.22) as the floor. It still needs version-specific handling and would be the only CI stack on that line.
- Ship only Flatpak. Native Arch and Debian packages remain the primary 0.2 targets.
