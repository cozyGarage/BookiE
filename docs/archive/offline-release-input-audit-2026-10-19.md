# Offline release input audit, 2026-10-19

## Current evidence

`scripts/build-arch-rc.sh` refuses a dirty tree, requires the candidate to resolve to `HEAD`, creates a Git archive from that immutable commit, computes its SHA-256, and passes both into the Arch recipe. `scripts/tests/test_arch_candidate.py` verifies those properties and rejects a non-SHA candidate, conflicting tag/SHA inputs, an unsafe version and a dirty tree. `validate-arch-package.sh` checks installed executables, schemas, desktop metadata and the BookiE license.

The Arch recipe still runs `cargo fetch --locked` in `prepare()` and invokes Cargo builds without `--offline`. The captured input is therefore the project source archive, not a source-plus-dependency archive. This does not prove a build can finish after registry access is removed. Native package dependencies are resolved separately by makepkg and are also not captured by the Cargo archive.

The app manifest reports `0.1.6-dev`, while candidate package version is an explicit environment input to the RC helper. The helper validates version syntax but does not compare it with the app manifest or a release date. This is useful for unpublished candidates, but it leaves consistency checks to the maintainer.

## Adoption decision

Extend the candidate pipeline in a later implementation PR to emit a provenance manifest alongside the source archive: commit, app version, candidate package version, date, Rust toolchain, target, selected workspace packages/features, vendor archive checksum, and declared native build inputs. Compare candidate metadata before packaging and fail on unexplained mismatches.

Capture Cargo registry and git dependencies into a verified vendor input, then run build and test commands with Cargo offline mode after preparation. Keep native system packages as declared external inputs; do not claim the whole build is offline unless those are also captured. Stable inputs and checksums prove provenance, not byte-for-byte reproducible binaries.

Extend the existing candidate fixture for wrong tags, version/date mismatch, checksum mismatch, absent runtime helpers and unsupported inputs. Keep installed Arch acceptance and the publication decision as separate evidence. This audit does not create a tag or publish the completed 0.2.0 release.

## Remaining implementation

The current immutable source archive and package validator remain useful. This record does not add vendor archives, offline Cargo builds, metadata checks or new package fixtures. Issue #1146 remains open until those changes land and pass the selected package gates.
