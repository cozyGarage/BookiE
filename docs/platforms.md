# Platforms, toolchains and packaging

BookiE is a native Linux desktop application. Current source builds require GTK 4.24, libadwaita 1.10, GtkSourceView 5.22 and GLib 2.88 or newer ([ADR 0021](decisions/0021-toolkit-floor.md)). Arch Linux and Debian testing provide the native toolkit floor. Older native toolkit versions are unsupported.

## Published artifacts

Current releases publish an Arch package and an AppImage on the [GitHub releases](https://github.com/cozyGarage/BookiE/releases) page. Artifact names follow `bookie-<version>-1-x86_64.pkg.tar.zst` and `BookiE-<version>-x86_64.AppImage`.

| Target | Architecture | Artifact pattern | Install or launch |
|---|---|---|---|
| Arch Linux | x86_64 | `bookie-<version>-1-x86_64.pkg.tar.zst` | Install with `pacman` |
| AppImage | x86_64 | `BookiE-<version>-x86_64.AppImage` | Run the downloaded file |

From 0.2.5 the release workflow no longer publishes a Debian package ([ADR 0031](decisions/0031-stop-publishing-the-debian-package.md)) or a Flatpak bundle ([ADR 0032](decisions/0032-stop-publishing-the-flatpak.md)). The [0.2.4 release](https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.2.4) still carries both (`bookie.flatpak` and a `.deb`). The workflow builds the Arch package against current Arch packages and the AppImage against Debian testing. Current source requirements are listed above.

The Arch package uses the host GTK 4 and libadwaita stack. The AppImage bundles Debian-testing GTK and libadwaita through linuxdeploy, so chrome and spacing can differ from an Arch install on the same machine. The AppImage launch path does not force `GTK_THEME=Adwaita`, so libadwaita CSS and the application colour-scheme preference apply.

The AppImage requires glibc 2.43 or newer. Ubuntu 24.04 LTS provides glibc 2.39 and is below this floor ([Ubuntu package](https://packages.ubuntu.com/noble/libc6)); Ubuntu 26.04 LTS provides glibc 2.43 ([Ubuntu package](https://packages.ubuntu.com/resolute/libc6)). The AppImage build checks the versioned glibc symbols required by every bundled ELF file and fails when any requires a newer version than the floor in `scripts/appimage-glibc-floor.txt`. Passing this check establishes the glibc boundary only; desktop, portal, keyring and database acceptance remain tracked in [#2207](https://github.com/cozyGarage/BookiE/issues/2207).

AppImage launch smoke: `scripts/test-appimage-launch.sh <AppImage>` extracts the image, installs only desktop-level host libraries (X11, Mesa, fontconfig, freetype, harfbuzz, fribidi, Wayland client; no GTK 4, libadwaita or GtkSourceView) in `ubuntu:26.04`, `fedora:latest` and `archlinux:latest` containers, starts `AppRun` under Xvfb and requires `main window activated`. The 0.2.6 image passed on Ubuntu 26.04.1, Fedora 45 and Arch Linux on X11. On Ubuntu with no GtkSourceView files on the host, SQL tokens in the editor were still coloured, so highlighting does not depend on host GtkSourceView data. The image does not bundle those desktop-level libraries, so a bare container without them fails to load `libfribidi.so.0`. Start the application through `AppRun` (the AppImage itself). Running `usr/bin/bookie` directly with the bundled libraries first on `LD_LIBRARY_PATH` while the distribution's GTK 4 is installed overflows the stack at startup, because GTK then loads the distribution's modules (see [#2815](https://github.com/cozyGarage/BookiE/issues/2815)); `AppRun` sets `GTK_PATH` and `GTK_EXE_PREFIX` to the bundled modules.

The AppImage does not provide desktop-session services. BookiE uses GTK `FileDialog` for file operations; GTK relies on the session's XDG Desktop Portal interfaces for these dialogs. A running `xdg-desktop-portal` with a configured FileChooser backend must be available on the session bus. The dialog implementation and appearance depend on the desktop session ([GTK maintainer discussion](https://discourse.gnome.org/t/problem-with-gtk-file-dialog-gtk4/25568), [FileChooser portal interface](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html)).

Saved database passwords and SSH secrets use Secret Service on the session bus. The AppImage does not bundle or start a Secret Service provider. A session provider, such as GNOME Keyring or KeePassXC with Secret Service integration, must be running and unlocked to read or store saved secrets. BookiE reports keyring failures when the service is unavailable, locked or cannot be unlocked.

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

The AppImage is built on Debian testing and bundles the libraries it links, including OpenSSL from that build host, so it can run when the Arch package cannot install because of an OpenSSL soname mismatch. Prefer the Arch package on Arch or Omarchy when OpenSSL 4 is available; use the AppImage when the host OpenSSL does not match.

## Accessibility

The application uses GTK and libadwaita controls. Manual checks for keyboard navigation, screen readers, high contrast, text scaling and native desktop interaction are listed in the [manual verification checklist](manual-verification-0.2-features.md).
