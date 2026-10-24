# PostgreSQL sidebar catalog scaling

Related: [#580](https://github.com/cozyGarage/BookiE/issues/580).

The focused test builds 50,000 tables across 100 schemas, builds the sidebar tree, searches for every table, and verifies that all 50,200 tree nodes remain visible. The same test body was run against the Linux baseline and the optimized implementation.

| Source | Test duration |
| --- | ---: |
| Linux baseline `b3d892546faa792357f96b5ace5b146d9d3c9e28` | 8.15 s |
| Optimized `sidebar_tree.rs` blob `497e8a50dbbb0380856b4662771e40d0cb498064` | 0.07 s |

These are one-run debug test-harness durations on the same machine. Both use the exact same temporary test, including the exact visible-node assertion, and include synthetic data creation, tree construction and filtering, but exclude Cargo compilation. The test harness reported about 116 times lower runtime for the optimized code. This is synthetic CPU evidence, not an end-to-end UI result.

The baseline rebuilds schema groups by filtering the complete table and view lists for each schema. The optimized path groups tables and views during one pass, records parent node indexes while building the tree, and follows those indexes during search and collapsed-ancestor checks without rebuilding key strings or a per-search hash map.

## Limits

This is not an end-to-end GTK measurement and does not include database catalog loading, real metadata permissions, connection latency or main-loop responsiveness. It does not verify query scoping, restricted-role metadata correctness or MCP catalog scoping; those remain open in #580 and belong to the B3 driver handoff.

## Saved collapse regression

On baseline `b3d892546`, a saved collapse for `public` hides all tables and views when catalog refresh leaves only that named schema. The sidebar omits schema headings for a single schema, so there is no heading to expand. The regression `removing_other_schemas_keeps_the_remaining_catalog_visible_after_a_saved_schema_collapse` fails with an empty visible list instead of `Tables`, `users`, `Views`, and `recent`.

The optimized parent indexes follow only nodes present in the tree. This keeps those objects visible while preserving collapsible table and view groups. The regression was executed against the baseline before applying the fix; this is a behavior correction in addition to the synthetic scaling improvement.
