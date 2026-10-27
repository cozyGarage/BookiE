# Arch candidate for native Wayland checks

Candidate: `bookie-0.2.0-1832-x86_64.pkg.tar.zst`, built from [PR #1832](https://github.com/cozyGarage/BookiE/pull/1832), source commit `db41aac69d849f6c177ba1a53212706805716159`. It contains the policy-refusal fix. Other unmerged workspace and export changes are outside this build.

The candidate built with `scripts/build-arch-rc.sh` using the default `dist` profile, eight Cargo jobs and the offline vendor archive. Rust and Cargo are 1.98.1, matching the repository's Rust 1.98 channel. The build host has GTK 4.24.1, libadwaita 1.10.0 and GtkSourceView 5.22.0. The local rustup package does not satisfy makepkg's versioned Rust package dependency, so the documented builder dependency override was used after verifying the installed compiler and other declared dependencies. No global toolchain was changed.

Build and package checks passed: 3,053 tests passed, 246 were ignored across 26 targets, and desktop-entry/AppStream validation passed. The generated package reports `bookie 0.2.0-1832`. Package archive integrity and content validation passed. Namcap completed with warnings only: the package has an unused loader entry, several dependencies are implicitly satisfied, `libsecret` may be unused, and the recipe has no Maintainer tag.

The initial packaging wrapper ended with status 1 in its finalizer. Namcap's default temporary directory hit this user's temporary-storage quota; setting `TMPDIR` to the candidate's on-disk validation directory resolved it. The recipe parser also runs with a restricted environment that drops the builder variables. A checked copy with literal source archive names passed namcap; the build recipe itself was not modified. These finalizer defects are recorded in [issue #1836](https://github.com/cozyGarage/BookiE/issues/1836). The documentation checker scanning ignored package output is recorded in [issue #1834](https://github.com/cozyGarage/BookiE/issues/1834).

Five packaged-executable scenarios passed under private Xvfb and D-Bus sessions: malformed SQL refusal, dismissed approval refusal, audit-journal-loss refusal, and refusal/recovery for broken-link and malformed policy files. The scenario log is retained with this evidence. This is automated display testing only; native Wayland checks have not been performed.

The downloadable package and its checksum files are in `/home/trung/Projects/bookie-artifacts/pr-1832/`. Verify `SHA256SUMS`, then install the candidate package with pacman. The build input manifest and exact source/vendor archive hashes are recorded there. This is a local candidate, not a published release.

Continue the [native manual verification checklist](../../manual-verification-0.2-features.md) after installation. Focus on editor focus after sidebar search, closing the final editor tab, narrow connection-card layout, approval-dialog rendering and keyboard behavior, and result replacement/close resource use. All remain unverified on native Wayland.
