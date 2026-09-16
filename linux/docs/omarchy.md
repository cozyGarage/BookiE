# BookiE 0.1.2 on Arch Linux Omarchy

Install from the `linux` branch. Do not mix this with 0.2.0. Do not package from a dirty tree.

The package is an internal Arch candidate, not an AUR or Flathub release. Visible name is BookiE. Commands `bookie` and `bookie-agentd` are installed, with `tablepro` and `tablepro-agentd` aliases. Application ID, D-Bus, XDG paths, and keyring schema stay `com.tablepro.linux` / `tablepro`.

## What this line includes

- PostgreSQL INTERVAL, INET, CIDR, and LSN render as text. Decode failures stay errors, not fake NULLs.
- A SQLite TEXT column that holds a blob stays display-only. Committing no longer overwrites it with `<N bytes>`.
- Jump to Column (Ctrl+Shift+J), complete SQL drafts, dialect-safe formatting, BookiE name and icon, `bookie` package commands.
- Toolchain is Rust 1.98 (`rust-toolchain.toml`).

Stay off 0.2.0: lossless typed values, GNOME 50, SQLx 0.9, Meson/GResource.

## Toolchain

Arch's `rust` package ignores `rust-toolchain.toml`. Use `rustup`. The two packages conflict. See [toolchains.md](toolchains.md).

```bash
sudo pacman -S --needed base-devel pkg-config gtk4 libadwaita \
  gtksourceview5 openssl libsecret krb5 clang rustup \
  desktop-file-utils appstream namcap gettext

rustup toolchain install 1.98.0 --profile minimal --component rustfmt,clippy
rustup toolchain install stable --profile minimal --component clippy
rustup default stable
```

Inside this directory, rustup selects 1.98 from the repo pin.

```bash
rustc --version
cargo --version
```

`rustc --version` should report 1.98.x.

## Checkout

Use a clean clone or a clean worktree. Do not use a tree that has 0.2.0 work or uncommitted files.

```bash
git clone https://github.com/cozyGarage/TablePro.git
cd TablePro
git fetch origin linux
git checkout linux
git pull origin linux
git status
git rev-parse HEAD
```

`scripts/build-arch-rc.sh` refuses a dirty tree and refuses a SHA that is not `HEAD`. It archives only `linux/` from the repo root. Record the SHA you build.

## Build the package

From `linux/`:

```bash
TABLEPRO_RC_COMMIT="$(git rev-parse HEAD)" TABLEPRO_RC_VERSION=0.1.2 ./scripts/build-arch-rc.sh
```

Do not set `TABLEPRO_RC_TAG` until a `linux-v…` tag exists. The helper runs `makepkg --cleanbuild`, `namcap`, and package-content checks. The package is `packaging/arch/bookie-0.1.2-1-x86_64.pkg.tar.zst`.

`makepkg --cleanbuild` builds from the commit archive. It does not reuse a dirty `target/` directory.

## Install

```bash
sudo pacman -U packaging/arch/bookie-0.1.2-1-x86_64.pkg.tar.zst
```

Launch with `bookie` or the BookiE desktop entry. `tablepro` still works.

```bash
desktop-file-validate /usr/share/applications/com.tablepro.linux.desktop
appstreamcli validate --no-net /usr/share/metainfo/com.tablepro.linux.metainfo.xml
bookie-agentd --help
```

## After install

- Saved connections, drafts, favorites, and the audit journal stay in `~/.config/tablepro` and `~/.local/share/tablepro`.
- Stop BookiE before upgrading or rolling back the package. Do not delete those directories or keyring records.
- If you ever migrated drafts, `workspace_state.before-drafts.json` is the backup of the pre-migration workspace.
- The package replaces and conflicts with `tablepro`. Test install, upgrade, and removal. User XDG data must remain.
- Tagging `linux-v0.1.2` and publishing remain a separate decision.

## Run from source without packaging

```bash
cargo run --release -p tablepro-app
```

PostgreSQL container fixtures need Docker. They run in GitHub Actions on `linux`.
