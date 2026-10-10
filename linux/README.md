# BookiE for Linux

The product, installation, build and contributor guide lives at the repository root: [README.md](../README.md).

This directory is the Cargo workspace root (`Cargo.toml`) and holds the Linux application crates, packaging, scripts and docs. Start here for `cargo` commands when you are already inside `linux/`:

```bash
cargo run -p tablepro-app
./scripts/preflight.sh
```

| Topic | Document |
|---|---|
| Documentation map | [docs/README.md](docs/README.md) |
| Crate map, pipeline, where new code goes | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Native library floors (build/CI vs package line) | [docs/platforms.md](docs/platforms.md), [ADR 0002](docs/decisions/0002-rust-gtk4-libadwaita.md) |
| ADRs | [docs/decisions/README.md](docs/decisions/README.md) |
| Packaging | [packaging/README.md](packaging/README.md) |
