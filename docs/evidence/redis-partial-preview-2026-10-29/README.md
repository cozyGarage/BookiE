# Redis partial string previews

Implementation source: `a7eaf5651`, including current Linux and the partial-cell consumers from [#1955](https://github.com/cozyGarage/BookiE/pull/1955). Review and implementation belong to [#1180](https://github.com/cozyGarage/BookiE/issues/1180) and [#1903](https://github.com/cozyGarage/BookiE/pull/1903).

## Reproduction and correction

The native boundary regression failed on the previous preview implementation: a 4,097-byte string was returned as complete text after reading only 4,096 bytes. String length and the bounded prefix are now read in one Redis transaction. A value of at most 4,096 bytes retains its complete text or binary representation. A longer value uses the existing explicit partial-cell descriptor, whose encoder and parser now live in core. App-services retains its public parser export for existing callers.

Stored marker-shaped text remains ordinary text. A binary prefix is an explicit partial cell with a byte-oriented display prefix, not a complete binary value. Full-value refetch retains the original exact value. The imported GTK partial-cell handling blocks incomplete row copy/export and supports on-demand refetch. This does not add a second marker format or a driver dependency on app-services.

## Validation scope

- The new native boundary test failed before the fix and passed afterward. It covers complete 4,096-byte text, longer text, longer binary data and ordinary marker-shaped text.
- All 14 Redis native integration cases passed before the Linux refresh, including full-value refetch.
- The shared core descriptor round-trip and malformed-descriptor test passed.
- The GTK row-object test that blocks row copy/export for partial cells passed using the shared encoder.
- Full preflight passed on the refreshed implementation: 2,317 unit tests and 314 sandbox tests, plus harness, guards, formatting and Clippy.
- The complete isolated GTK widget suite passed: 211 tests.
- Published validation head: `afa97c9f5`. The following evidence-only commit does not change tested code.

The unchanged collection previews remain unbounded by member bytes. HSCAN/SSCAN count hints do not establish a hard memory limit, and lists or sorted sets can contain large elements. This correction does not resolve that separate finding. Native Wayland and Forgejo acceptance were not run.
