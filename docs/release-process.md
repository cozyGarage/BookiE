# Release process

A release is a tag named `linux-vX.Y.Z` on the protected `main` branch. The GitHub release workflow validates the source version, reads the committed `CHANGELOG.md` section and builds the release artifacts.

## Before tagging

Finish the selected [release scope](backlog.md#finishing-028) before cutting a candidate. A merged change and a passing unit test do not replace installed runtime acceptance.

1. Merge the reviewed release change to `main`. It sets the workspace version, Meson version and newest AppStream entry to the same `X.Y.Z` value, regenerates `Cargo.lock` with `cargo update --workspace` and `apps/linux/packaging/THIRD_PARTY_NOTICES.md` with `python3 scripts/update-third-party-notices.py` (preflight fails on stale notices once the version changes), and adds a `## [X.Y.Z]` section to `CHANGELOG.md`. Run `python3 scripts/check-release-version.py` and `python3 scripts/release-notes.py notes X.Y.Z`.
2. Write that section from `changelog.d` fragments with `python3 scripts/release-notes.py release X.Y.Z YYYY-MM-DD`; it moves them into `CHANGELOG.md` and deletes them ([ADR 0053](decisions/0053-changelog-fragments.md)). Only the release pull request edits `CHANGELOG.md`. The release workflow reads the committed section with `release-notes.py notes <version>` and fails when it is missing or empty. A wrong or missing fragment is fixed with a normal commit before the release is cut.
3. Confirm the merge commit is on `main` and required checks apply to its exact revision.

## Candidate runtime and benchmark baseline

Use one immutable source revision and checksum-identified package for a candidate pass. Record the source, package, toolchain, engine versions, fixture sizes, platform/session and commands in private verification records. Keep clean and dirty source runs separate. If a fix changes the candidate, re-run the affected acceptance layers and start a new measurement record.

Follow the [release qualification rules](validation-playbook.md#release-qualification), [manual checklist](manual-verification-0.2-features.md) and [testing and profiling procedures](testing.md#real-driver-integration-tests). Verify connect, browse, edit/save, query/cancel, import/export, session switching and close on disposable data before interpreting performance results. Run the relevant driver and release layers; an Xvfb pass does not certify native Wayland, focus or accessibility.

Measure cold and warm startup, first-page latency, large-catalog search, wide/large-cell results and repeated run/pin/export/close retention separately. Report median and tail latency, process RSS/peak and decoded-payload estimates as distinct quantities. Use the same source, dataset, engine settings and run count for comparisons. Driver-only or headless measurements do not establish GUI responsiveness. Source-pinned CI passes remain required; benchmarks do not replace correctness checks.

## Build and publish

1. Create and push an annotated `linux-vX.Y.Z` tag on the merged `main` commit.
2. Wait for the release workflow. It rejects a source version mismatch or incomplete release notes, builds the packages and drafts the GitHub release.
3. Review the draft and release-note categories. Verify each asset against `SHA256SUMS` and the build provenance. Install and launch at least one package before publishing.
4. Publish the reviewed draft. Do not substitute pull-request artifacts for tag-run assets.

Artifact naming for the Arch package and AppImage, the historical 0.2.4 Debian and Flatpak assets, and installation notes are in the [platform guide](platforms.md).

## After publishing

The tag run's `AUR recipe` job runs `scripts/update-aur-recipe.py` for the new tag and uploads the result as the `aur-recipe` workflow artifact. The file has `pkgver`, `pkgrel=1` and `_commit` (the commit the tag points at) set from the tag and nothing else changed. To refresh the in-tree copy, run `python3 scripts/update-aur-recipe.py linux-vX.Y.Z` and open a pull request, or download the artifact. Then build it with `makepkg` and push the same file to the AUR repository. Nothing in CI pushes to the AUR and CI holds no AUR key.

## Recovering publication after successful package builds

The release workflow supports recovery when package builds succeed but draft creation fails. Dispatch `release.yml` with `artifact_run_id` and `release_tag` set to the original tag run. The recovery path validates the run and tagged commit, reuses only that run's artifacts, regenerates checksums and provenance, and does not move the tag or rebuild packages. Verify the original build identity and the workflow revision that finalized the assets.

## Fixing a bad release

Delete or unpublish the draft, fix the source on `main`, and cut the next patch version. Do not move a tag that users may have downloaded.
