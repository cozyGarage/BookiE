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

The AppImage requires glibc 2.43 or newer. Ubuntu 24.04 LTS provides glibc 2.39 and is below this floor ([Ubuntu package](https://packages.ubuntu.com/noble/libc6)); Ubuntu 26.04 LTS provides glibc 2.43 ([Ubuntu package](https://packages.ubuntu.com/resolute/libc6)). The AppImage build checks the versioned glibc symbols required by every bundled ELF file and fails when any requires a newer version than the floor in `scripts/appimage-glibc-floor.txt`. Passing this check establishes the glibc boundary only; desktop, portal, keyring and database acceptance remain tracked in [#2207](https://github.com/cozyGarage/BookiE/issues/2207).

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

## OpenSSL

The Arch package links OpenSSL 4 (`libssl.so.4`, `libcrypto.so.4`) and depends on those sonames (`libssl.so=4-64`, `libcrypto.so=4-64`), so a system still on OpenSSL 3 cannot install it. Builds from source need OpenSSL 4 development headers.

## Accessibility

The application uses GTK and libadwaita controls. Manual checks for keyboard navigation, screen readers, high contrast, text scaling and native desktop interaction are listed in the [manual verification checklist](manual-verification-0.2-features.md).
