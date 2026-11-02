# CI checks

This page summarizes the contributor-facing CI contract. Test tiers and local commands are listed in the [validation playbook](validation-playbook.md#ci-tiers).

Required pull-request checks apply to the exact proposed commit. Passing a fast check does not certify test tiers it did not run. A skipped, pending or failed required check is not a pass. Maintainers confirm acceptance and handle merging.

CI results apply only to the source revision they tested. Keep fixtures and test inputs isolated, and include any needed test or workflow changes in the pull request.

Forgejo build-host jobs keep separate, job-specific Cargo targets on each builder's private persistent volume. Cargo reuses unchanged dependency outputs, including the DuckDB native build, across runs on that builder. Each run removes its temporary directory; the cache helper prunes only recognized generated profile outputs when they age past five days or the volume needs its 30 GiB reserve.
