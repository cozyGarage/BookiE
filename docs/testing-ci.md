# CI checks

This page summarizes the contributor-facing CI contract. Test tiers and local commands are listed in the [validation playbook](validation-playbook.md#ci-tiers).

GitHub runs the required pull-request checks for the exact proposed commit, including the cheap validation tier and Driver TLS fixture. Forgejo remains the acceptance gate for the remaining test tiers. A skipped, pending or failed required check is not a pass. Maintainers handle Forgejo acceptance and merging.

CI results apply only to the source revision they tested. Keep fixtures and test inputs isolated, and include any needed test or workflow changes in the pull request.
