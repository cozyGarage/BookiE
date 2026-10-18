# Third-party notice inventory, 2026-10-19

## Current package contents

The current Arch recipe installs BookiE's `LICENSE.md` under `/usr/share/licenses/bookie/`. It does not install a dependency notice bundle. The Debian recipe and validator still target the released 0.1.5 package; Debian is not a 0.2.0 candidate target. `deny.toml` checks license policy, which does not provide the license texts shipped in a package.

## Dependency metadata snapshot

At the inspected `linux` source revision, `cargo metadata --format-version 1 --locked` returned 718 package records across the workspace lock graph. All 718 declare an SPDX `license`; none declares Cargo's `license_file` field. This includes packages outside the shipped application graph, so the count is not a notice count. It confirms that SPDX metadata alone cannot satisfy the notice requirement and that the inventory must first resolve each shipped feature/target graph.

## Adoption decision

Implement a pinned generator against the exact Arch package feature graph, with separately declared GUI, agentd and askpass roots and an explicit decision about optional DuckDB. Preserve original license text and provenance in generated output. A generator must fail when a selected package's license text cannot be resolved; accepting SPDX identifiers alone would produce an incomplete notice bundle. Native system libraries need a separate package-level attribution source because Cargo metadata does not describe their installed license texts.

Keep the generated bundle out of runtime resources. Install it under the package's documentation/license directory and expose it through package documentation or About. Add regeneration drift checks to the cheap supply-chain harness and package-content assertions to existing package tiers.

## Remaining implementation

This record establishes the current gap and the required design. It does not generate or ship third-party notices. The linked issue remains open until the generator, provenance checks, package integration and drift tests land.
