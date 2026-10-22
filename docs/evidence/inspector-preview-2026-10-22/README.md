# Inspector preview resource investigation

Tracker: [#1370](https://github.com/cozyGarage/BookiE/issues/1370).
[Source and trace hashes](measurements.json) identify the matched builds.

## Verified mechanism and change

The single-row inspector materialized every cell before checking preview
metadata. It formatted values and then overwrote previewed fields. Text output
was already bounded, but input scanning and full row cloning still occurred;
JSON formatting can serialize the full value. The selection callback performs
this work even when the inspector sidebar is hidden.

The candidate reads existing previews first and borrows remaining cells. It
preserves field names/types, null and missing distinctions, preview byte counts
and exact stored values. Hidden widget construction is unchanged.

## Matched headless measurement

Both debug binaries use the same baseline and test. The baseline adds only the
fixture. It uses an integer key, 8 MiB text, 8 MiB binary, a null and an unfetched
cell. Eight refreshes assert preview output and intact source text. The fixture
pins existing behavior; it does not assert an allocation threshold.

| Metric | Baseline | Candidate |
| --- | ---: | ---: |
| Peak heap, MB | 33.86 | 17.08 |
| Peak RSS including Heaptrack, MB | 85.44 | 65.73 |
| Instrumented process time, seconds | 0.21 | 0.05 |

Three alternating uninstrumented pairs are in [direct measurements](direct-measurements.json).
Elapsed process time was 0.13–0.16 seconds before and 0.05–0.07 after. Peak
RSS was 76,932–78,592 KiB before and 61,340–61,560 after. These are short debug
fixture runs on a shared host, including process startup, not whole-app or
native desktop latency claims. No display or database is needed by this fixture.

Raw traces, saved binaries, reports and logs remain under
`~/bookie-test/inspector-preview/`. To reproduce the measurement, build the test
binary for each source, save it, then run `heaptrack --record-only` with
`--exact ui::row_inspector::tests::wide_row_fields_preserve_previews_nulls_missing_cells_and_source_values --test-threads=1`.
Use `heaptrack_print -f <trace>` to inspect the captured allocation report.

## Validation

The fixture passed on both versions. All three inspector unit tests pass on the
candidate. Full preflight passed, including 2,053 non-GTK unit tests and the
sandbox tier. All 719 application unit tests and 104 isolated GTK widget
tests pass. All-target Clippy, documentation links and the ignored-test inventory pass. Forgejo
acceptance and native desktop responsiveness are not yet checked.

## Separate lifecycle candidate

The browse selection callback captures the inspector strongly. The inspector
owns its split-view content, which contains the grid and selection model. This
is a possible ownership cycle, recorded on [#1333](https://github.com/cozyGarage/BookiE/issues/1333).
It is not a confirmed leak. A regression must retain weak references to the
actual browse selection and inspector, remove the browse component, pump its
main context and check release. A plain native control must release under the
same conditions before attributing a failure to application callbacks. This
preview-allocation change does not alter that graph.
