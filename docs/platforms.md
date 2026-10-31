# Platforms, toolchains and packaging

BookiE is a native Linux desktop application. Current source builds require GTK 4.24, libadwaita 1.10, GtkSourceView 5.22 and GLib 2.88 or newer ([ADR 0021](decisions/0021-toolkit-floor.md)). Arch Linux and Debian testing provide the native toolkit floor. Older native toolkit versions are unsupported.

## Published 0.2.0 artifacts

The [GitHub release](https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.0) provides these Linux artifacts:

| Target | Architecture | Artifact | Install or launch |
|---|---|---|---|
| Arch Linux | x86_64 | `bookie-0.2.0-1-x86_64.pkg.tar.zst` | Install with `pacman` |
| Debian | amd64 | `tablepro_0.2.0-1_amd64.deb` | Install with `apt`; command is `bookie` |
| AppImage | x86_64 | `BookiE-0.2.0-x86_64.AppImage` | Run the downloaded file |
| Flatpak bundle | x86_64 | `bookie.flatpak` | Install with `flatpak` |

The published binaries were built before the current native toolkit floor. Build from the current `linux` branch to use current source and dependencies.

## Rust toolchain

Rust 1.98 is the minimum supported version, declared in `rust-toolchain.toml`, `Cargo.toml` and `clippy.toml`. Use rustup to select the repository toolchain. On Arch, the `rust` and `rustup` packages conflict; install `rustup` instead:

```bash
sudo pacman -S rustup
rustup toolchain install 1.98.0 --profile minimal --component rustfmt,clippy
rustup toolchain install stable --profile minimal --component clippy
rustup default stable
```

In this repository, the pin selects 1.98. Use `cargo +stable clippy ...` for the forward-compatibility check.

## Package builds

Use the [packaging guide](../apps/linux/packaging/README.md) to build Arch, Debian or Flatpak packages from the current source. The Arch and Debian builders retain checksummed sources and validate package contents. Their commands do not create a release tag.

The current Flatpak manifests use the GNOME 51 runtime. Use `scripts/build-flatpak.sh` to produce a local bundle.

The Arch package installs `bookie` and `bookie-agentd`, with `tablepro` compatibility aliases. The Debian package installs the same commands. Both preserve the existing application ID, configuration and data paths, keyring schema and protocol identifiers.

## Accessibility

The application uses GTK and libadwaita controls. Manual checks for keyboard navigation, screen readers, high contrast, text scaling and native desktop interaction are listed in the [manual verification checklist](manual-verification-0.2-features.md).
