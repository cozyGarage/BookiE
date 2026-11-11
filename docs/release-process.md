# Release process

A release is a tag named `linux-vX.Y.Z` on the protected `main` branch. The GitHub release workflow validates the source version, assembles notes from squash-merge commit trailers and builds the release artifacts.

## Before tagging

1. Merge the reviewed release change to `main`. It sets the workspace version, Meson version and newest AppStream entry to the same `X.Y.Z` value, regenerates `Cargo.lock` with `cargo update --workspace` and `apps/linux/packaging/THIRD_PARTY_NOTICES.md` with `python3 scripts/update-third-party-notices.py` (preflight fails on stale notices once the version changes), and adds a `## [X.Y.Z]` section to `CHANGELOG.md`. Run `python3 scripts/check-release-version.py` and `python3 scripts/release-notes.py notes X.Y.Z`.
2. From the release after [ADR 0053](decisions/0053-changelog-fragments.md) lands, write the section from `changelog.d` fragments with `python3 scripts/release-notes.py release X.Y.Z YYYY-MM-DD`; it moves them into `CHANGELOG.md` and deletes them. Until the trailer path is retired, a change may still carry a `Release-Note:` line. Otherwise, build that changelog section from the `Release-Note:` trailers with `python3 scripts/release-notes.py assemble --since <previous tag> --until HEAD`, and fix by hand what the trailers got wrong. A squash commit on `main` cannot be amended, so a commit with no trailer or an unknown category is reported and must be written or dropped on purpose, and a note that names something the release no longer ships must be corrected. Pull requests carry a `Release-Note: <Category>: <text>` line, or `Release-Note: none`, and the category is one of Added, Changed, Deprecated, Removed, Fixed or Security. Only the release pull request edits `CHANGELOG.md`. The release workflow reads the committed section with `release-notes.py notes <version>` and fails when it is missing or empty. [ADR 0022](decisions/0022-release-notes-from-merge-commits.md) describes assembling the notes from trailers at release time; the workflow does not do that yet.
3. Confirm the merge commit is on `main` and required checks apply to its exact revision.

## Build and publish

1. Create and push an annotated `linux-vX.Y.Z` tag on the merged `main` commit.
2. Wait for the release workflow. It rejects a source version mismatch or incomplete release notes, builds the packages and drafts the GitHub release.
3. Review the draft and release-note categories. Verify each asset against `SHA256SUMS` and the build provenance. Install and launch at least one package before publishing.
4. Publish the reviewed draft. Do not substitute pull-request artifacts for tag-run assets.

The Arch and AppImage artifact names (0.2.4 also has Debian and Flatpak) and installation notes are in the [platform guide](platforms.md).

## Recovering publication after successful package builds

The release workflow supports recovery when package builds succeed but draft creation fails. Dispatch `release.yml` with `artifact_run_id` and `release_tag` set to the original tag run. The recovery path validates the run and tagged commit, reuses only that run's artifacts, regenerates checksums and provenance, and does not move the tag or rebuild packages. Verify the original build identity and the workflow revision that finalized the assets.

## Fixing a bad release

Delete or unpublish the draft, fix the source on `main`, and cut the next patch version. Do not move a tag that users may have downloaded.
