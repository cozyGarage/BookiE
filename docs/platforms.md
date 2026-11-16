# Platforms, toolchains and packaging

BookiE is a native Linux desktop application. Current source builds require GTK 4.24, libadwaita 1.10, GtkSourceView 5.22 and GLib 2.88 or newer ([ADR 0021](decisions/0021-toolkit-floor.md)). Arch Linux and Debian testing provide the native toolkit floor. Older native toolkit versions are unsupported.

## Published 0.2.4 artifacts and later releases

The [GitHub release](https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.4) provides these Linux artifacts:

| Target | Architecture | Artifact | Install or launch |
|---|---|---|---|
| Arch Linux | x86_64 | `bookie-0.2.4-1-x86_64.pkg.tar.zst` | Install with `pacman` |
| AppImage | x86_64 | `BookiE-0.2.4-x86_64.AppImage` | Run the downloaded file |
| Flatpak bundle (0.2.4 only) | x86_64 | `bookie.flatpak` | Install with `flatpak` |

From 0.2.5 the release workflow no longer publishes a Debian package ([ADR 0031](decisions/0031-stop-publishing-the-debian-package.md)) or a Flatpak bundle ([ADR 0032](decisions/0032-stop-publishing-the-flatpak.md)); the 0.2.4 release above still carries both. The workflow builds the Arch package against current Arch packages and the AppImage against Debian testing. Current source requirements are listed above.

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

Use the [packaging guide](../apps/linux/packaging/README.md) to build Arch packages from the current source. The Arch builder retains checksummed sources and validates package contents. Its command does not create a release tag. The Debian builder remains as an internal staging step for the AppImage.

The Arch package installs `bookie`, `bookie-agentd` and `bookie-askpass`. It replaces the legacy `tablepro` package; the first start copies legacy state, as described in the [storage guide](storage.md).

## Community AUR recipe

`apps/linux/packaging/aur/PKGBUILD` builds the released source of one `linux-vX.Y.Z` tag on the user's own machine. The release `.pkg.tar.zst` is built in CI against current Arch packages, so it needs the same library versions that image had; the AUR recipe compiles against whatever is installed and only needs the toolkit floor above, so it is the path for an Arch system that has not caught up. Both install `bookie`, `bookie-agentd` and `bookie-askpass`, keep the application ID and keyring schema, and replace the legacy `tablepro` package.

The recipe follows the tag and pins the commit the tag must resolve to; a moved tag fails the build. It checks GTK 4.24, libadwaita 1.10, GtkSourceView 5.22 and GLib 2.88 with `pkg-config` before compiling and stops with the version it found. Release tags are annotated but not yet GPG-signed, so the recipe cannot use `?signed` until the maintainer signs them. The recipe holds no secrets and downloads only the tag and the locked crates.

It is not published until the maintainer pushes it to the AUR; the in-tree file is the source for that repository and is updated after each release ([release process](release-process.md)).

## Accessibility

The application uses GTK and libadwaita controls. Manual checks for keyboard navigation, screen readers, high contrast, text scaling and native desktop interaction are listed in the [manual verification checklist](manual-verification-0.2-features.md).
