# CI checks

This page summarizes the contributor-facing CI contract. Test tiers and local commands are listed in the [validation playbook](validation-playbook.md#ci-tiers).

Required pull-request checks apply to the exact proposed commit. Passing a fast check does not certify test tiers it did not run. A skipped, pending or failed required check is not a pass. Maintainers confirm acceptance and handle merging.

CI results apply only to the source revision they tested. Keep fixtures and test inputs isolated, and include any needed test or workflow changes in the pull request.
