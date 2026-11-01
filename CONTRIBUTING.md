# Contributing to BookiE

BookiE is a Linux-only Rust and GTK database client. Current development happens on the `linux` branch. Pull requests target `linux` in [`cozyGarage/BookiE`](https://github.com/cozyGarage/BookiE).

## Start here

- Read [`AGENTS.md`](AGENTS.md) for contributor rules, lane ownership, tests, security, commits and review workflow.
- Read the [documentation map](docs/README.md) and choose work from [Project #3](https://github.com/users/cozyGarage/projects/3). Each issue lists its acceptance criteria.
- The Cargo workspace is at the repository root. [Architecture](ARCHITECTURE.md) explains crate boundaries and data flow.

## Development setup

Install the native libraries and Rust toolchain listed in the [README](README.md) and [platform guide](docs/platforms.md). Replace `OWNER` with your GitHub account, then clone your fork and check the environment:

```bash
git clone https://github.com/OWNER/BookiE.git
cd BookiE
git checkout linux
rustc --version
pkg-config --modversion gtk4 libadwaita-1 gtksourceview-5
cargo run -p tablepro-app
```

## Validation

Use the [validation playbook](docs/validation-playbook.md) to select local checks for your change and identify the applicable required checks. Do not treat a skipped check as a pass.

## Reporting bugs

Open a [bug report](https://github.com/cozyGarage/BookiE/issues/new?template=bug_report.yml) with the BookiE version, Linux distribution, desktop session, reproduction steps, database type and version, and redacted logs. Never attach credentials, tokens, connection strings, SQL parameters or unmasked database values.

Contributions are licensed under [AGPL-3.0-or-later](LICENSE).
