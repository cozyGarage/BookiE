# Verbatim text value viewing, 2026-10-27

Issue #1744, lane UX. Source base: `887678414`. This change affects the read-only value viewer, not decoders, bindings or stored values.

The viewer previously parsed text beginning with `{` or `[` into a JSON object and pretty-printed it. A regression failed on the original code: `{"a":1,"a":2}` lost its first key. Parsing could also alter large numbers, exponent spelling, escapes and whitespace.

The text path now uses the existing full-value text renderer directly. Typed JSON still uses its existing pretty display, and binary values retain their hex display. The display cap and complete-value copy callback are unchanged. No claim is made about restoring information already lost before a value reaches this viewer.

Seven pure viewer tests passed. The regression covers Text, JsonText and UTF-8 Bytes with duplicate keys, large decimal text, exponent spelling, escapes, Unicode and whitespace. Existing tests cover typed JSON, NULL, binary/UTF-8 bytes, long hex dumps and display truncation.

The existing installed scenario expected pretty-printing of a SQLite text literal. It is updated for the reported behavior: display that stored text verbatim, including duplicate keys and numeric spelling. Historical evidence retains its old scenario name and result.

Full preflight passed: 2,208 unit tests, 291 sandbox tests and one Redis check. App Clippy passed. The full local CI tier later passed 3,012 workspace unit tests, 291 sandbox tests and one Redis check. The rebuilt installed application passed `view_value_preserves_duplicate_json_keys_and_text_formatting` under isolated Xvfb, asserting the entire displayed buffer equals the SQLite text literal. The first installed attempt did not open the dialog through the inherited pointer/menu-position sequence. The final scenario uses the existing result-column keyboard navigation before opening the viewer. All 179 isolated GTK widget tests passed, including the full-value viewer test. Forgejo acceptance is pending; active gates must drain before this pull request can be scheduled. App-services generated artifacts were removed from debug and ci profiles before broad validation to avoid stale binaries from another worktree.

Release-Note: Fixed: The value viewer preserves JSON-looking text without dropping duplicate keys or reformatting numbers.

## Bounded preview allocation

Follow-up baseline: `9e415fdf5`. The viewer previously copied an entire value before applying its 1 MiB display cap. The process-isolated regression failed on that baseline: a 64 MiB Text fixture added 69,968 KiB to peak RSS. A later typed-JSON check found the same allocation in pretty serialization: 64 MiB of JSON text added 70,924 KiB.

Text, JsonText and valid UTF-8 bytes now copy only the bounded display prefix. Typed JSON is pretty-serialized through a writer that retains at most the same 1 MiB prefix while counting the serialized bytes. The dialog keeps the original value for its Copy button and only creates the full copy string after the user clicks it. Binary hex output and exact full-value copying remain available.

Each 64 MiB value runs in a fresh test process. The fixture stays alive during measurement; `/proc/self/status` VmHWM is sampled before and after rendering, with an allowed increase below 16 MiB. The current run measured extra peaks of 4,172 KiB for Text, 4,300 KiB for JsonText, 4,628 KiB for UTF-8 Bytes and 7,380 KiB for typed JSON. All nine pure viewer tests passed. These are synthetic debug-test process measurements, not whole-application or database throughput claims.

Current local validation passed: full preflight (2,208 unit tests, 291 sandbox tests and one Redis check), app Clippy, all 179 isolated GTK widget tests on fixed Xvfb display `:207` at 1600×1200, and the installed AT-SPI scenario `view_value_preserves_duplicate_json_keys_and_text_formatting`. The installed scenario used the rebuilt CI-profile binary and verified the stored text remained exact in the value viewer. Forgejo acceptance remains pending; other gate processes are active, and this pull request has not yet been queued.
