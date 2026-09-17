# Packaging

GitHub Release `linux-v0.1.4` ships an Arch/Omarchy `.pkg.tar.zst` and an Ubuntu 25.10 `.deb`.
Nothing here is ready for AUR or Flathub. BookiE keeps the existing repository and
application ID. Portal permissions and the public update channel still need release review.

## Internal Arch RC

The recipe in [`arch/PKGBUILD`](arch/PKGBUILD) accepts only an immutable commit
archive and a real SHA-256 checksum. It installs `bookie`, `bookie-agentd`, compatibility aliases `tablepro` and
`tablepro-agentd`, desktop/AppStream/icon metadata, the license, the example
policy, and any compiled translation catalogs. It deliberately does not ship a
systemd unit: stdio agentd is launched on demand by its MCP client.

After committing and validating the candidate (no published tag required):

```bash
TABLEPRO_RC_COMMIT="$(git rev-parse HEAD)" TABLEPRO_RC_VERSION=0.1.4 ./scripts/build-arch-rc.sh
```

The helper refuses a dirty tree or a candidate SHA other than `HEAD`. It archives
that exact local commit, computes the checksum, builds with `makepkg`, and runs
`namcap` and package-content validation. Set `TABLEPRO_RC_TAG=linux-v…` instead
of `TABLEPRO_RC_COMMIT` to verify an already published tag against the remote.
The Arch package replaces/conflicts with `tablepro`; it retains production
paths, application ID, gettext domain, keyring records, and legacy commands.
Publication remains a separate decision.

Also validate the installed artifact in a clean Arch VM or container:

```bash
desktop-file-validate /usr/share/applications/com.tablepro.linux.desktop
appstreamcli validate --no-net /usr/share/metainfo/com.tablepro.linux.metainfo.xml
dbus-run-session -- bookie
bookie-agentd --help
```

Test install, upgrade, downgrade, and removal. Package operations must leave
the user's XDG configuration, data, state, and keyring records untouched.

## Debian / Ubuntu package

The amd64 `.deb` on the `linux-v0.1.4` GitHub Release is built on Ubuntu 25.10.
The GNOME 50 baseline needs GTK 4.22, GLib 2.88, libadwaita 1.9 and GtkSourceView 5.18.
Use `flatpak/com.tablepro.linux.Devel.json` for an isolated development Flatpak.

```bash
DEB_VERSION=0.1.4-1 ./scripts/build-deb.sh
sudo apt install ./packaging/out/tablepro_0.1.4-1_amd64.deb
```

The package name stays `tablepro`. It installs `/usr/bin/bookie` and
`/usr/bin/bookie-agentd`, with both legacy command aliases. It does not
install or enable a user service.

## Flatpak development build

The Flatpak manifest remains a non-release CI build:

```bash
./scripts/build-flatpak.sh
```

It still needs offline Cargo sources, portal work, and a
filesystem-permission review before any Flathub submission. A successful
manifest build is not release evidence for the Arch RC.
