# Packaging

See [platforms](../../../docs/platforms.md) for released artifacts, native requirements and package support. This guide covers package builds and validation. BookiE keeps the existing repository and application ID.

## Arch package candidate

The recipe in [`arch/PKGBUILD`](arch/PKGBUILD) accepts only an immutable commit archive and a real SHA-256 checksum. It installs `bookie`, `bookie-agentd`, `bookie-askpass`, desktop/AppStream/icon metadata, the license, the example policy, and any compiled translation catalogs. It deliberately does not ship a systemd unit: stdio agentd is launched on demand by its MCP client.

After committing and validating the candidate (no published tag required), from the repository root:

```bash
BOOKIE_RC_COMMIT="$(git rev-parse HEAD)" \
BOOKIE_RC_VERSION="$(python3 scripts/check-release-version.py --print)" \
BOOKIE_RC_DATE="$(date -u +%F)" bash scripts/build-arch-rc.sh
```

The example builds the workspace release version from the committed tree. The helper refuses a dirty tree or a candidate SHA other than `HEAD`. It archives that exact local commit, computes the checksum, builds with `makepkg`, and runs `namcap` and package-content validation. Set `BOOKIE_RC_TAG=linux-v…` instead of `BOOKIE_RC_COMMIT` to verify an already published tag against the remote. The Arch package replaces/conflicts with `bookie`; it retains production paths, application ID, gettext domain, keyring records, and legacy commands. Publication remains a separate decision.

The standalone build, upgrade and removal wrapper runs in an isolated Arch container:

```bash
bash scripts/test-arch-package-container.sh
```

Its version defaults to the workspace version and must match it. The candidate date uses `ARCH_CANDIDATE_DATE`, then an inherited `BOOKIE_RC_DATE`, then the current UTC date. Pin `ARCH_CANDIDATE_DATE` when repeating a dated candidate. Invalid versions and calendar dates are refused before Docker starts. `ARCH_RELEASED_URL` selects the published upgrade fixture.

Package-content checks can run in a clean Arch VM or container. To check installed behavior, use a target Arch or Omarchy system running Hyprland on native Wayland. Run the UI smoke in that graphical session:

```bash
desktop-file-validate /usr/share/applications/com.bookie.linux.desktop
appstreamcli validate --no-net /usr/share/metainfo/com.bookie.linux.metainfo.xml
dbus-run-session -- bookie
bookie-agentd --help
```

Test install, upgrade, downgrade, and removal. Package operations must leave the user's XDG configuration, data, state, and keyring records untouched.

## Debian staging build

For native-library and toolchain requirements, see [platforms](../../../docs/platforms.md).

BookiE no longer publishes a Debian package ([ADR 0031](../../../docs/decisions/0031-stop-publishing-the-debian-package.md)). The Debian builder and validator remain because the AppImage build stages its files with them. They use the workspace release version with Debian revision `-1` by default, and `DEB_VERSION` can select another revision. The command below builds a local package for development only; the result needs a system with the toolkit versions listed in [platforms](../../../docs/platforms.md) and does not replace the manual scenarios in the [verification checklist](../../../docs/manual-verification-0.2-features.md).

```bash
DEB_VERSION=0.2.5-1 ./scripts/build-deb.sh
sudo apt install ./apps/linux/packaging/out/bookie_0.2.5-1_amd64.deb
```

The package name is `bookie`. It installs `/usr/bin/bookie`, `/usr/bin/bookie-agentd`, and `/usr/bin/bookie-askpass`. It replaces and conflicts with the legacy `tablepro` package. It does not install or enable a user service.

## Release versions

`[workspace.package].version` in the root `Cargo.toml` is the BookiE release version. Every workspace crate inherits it. For a release or hotfix, update that value, the Meson version and the newest AppStream entry together, then regenerate `Cargo.lock`. Keep older AppStream entries and published upgrade fixtures intact. The published 0.2.0 entry is marked `type="stable"` with release date 2026-10-23.

Run `python3 scripts/check-release-version.py` to check consistency; the script harness also runs it. `--print` returns the source version for packaging. Translation generation resolves the inherited version, and MCP advertises its compiled crate version. These version declarations do not change compatibility identifiers, create a tag or publish artifacts.
