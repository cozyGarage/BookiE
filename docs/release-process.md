# Release process

A release is a tag named `linux-vX.Y.Z` on the protected `linux` branch. The GitHub release workflow validates the source version, assembles notes from squash-merge commit trailers and builds the release artifacts.

## Before tagging

1. Merge the reviewed release change to `linux` with the workspace version, Meson version and newest AppStream entry set to the same `X.Y.Z` value. Run `python3 scripts/check-release-version.py`.
2. Confirm each user-facing squash commit has a `Release-Note: <Category>: <text>` trailer, or `Release-Note: none`. Do not edit `CHANGELOG.md`; see [ADR 0022](decisions/0022-release-notes-from-merge-commits.md).
3. Confirm the merge commit is on `linux` and required checks apply to its exact revision.

## Build and publish

1. Create and push an annotated `linux-vX.Y.Z` tag on the merged `linux` commit.
2. Wait for the release workflow. It rejects a source version mismatch or incomplete release notes, builds the packages and drafts the GitHub release.
3. Review the draft and release-note categories. Verify each asset against `SHA256SUMS` and the build provenance. Install and launch at least one package before publishing.
4. Publish the reviewed draft. Do not substitute pull-request artifacts for tag-run assets.

The Arch, Debian, Flatpak and AppImage artifact names and installation notes are in the [platform guide](platforms.md).

## Recovering publication after successful package builds

The release workflow supports recovery when package builds succeed but draft creation fails. Dispatch `release.yml` with `artifact_run_id` and `release_tag` set to the original tag run. The recovery path validates the run and tagged commit, reuses only that run's artifacts, regenerates checksums and provenance, and does not move the tag or rebuild packages. Verify the original build identity and the workflow revision that finalized the assets.

## Fixing a bad release

Delete or unpublish the draft, fix the source on `linux`, and cut the next patch version. Do not move a tag that users may have downloaded.
