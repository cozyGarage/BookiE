# Verbatim text value viewing, 2026-10-27

Issue #1744, lane UX. Source base: `887678414`. This change affects the read-only value viewer, not decoders, bindings or stored values.

The viewer previously parsed text beginning with `{` or `[` into a JSON object and pretty-printed it. A regression failed on the original code: `{"a":1,"a":2}` lost its first key. Parsing could also alter large numbers, exponent spelling, escapes and whitespace.

The text path now uses the existing full-value text renderer directly. Typed JSON still uses its existing pretty display, and binary values retain their hex display. The display cap and complete-value copy callback are unchanged. No claim is made about restoring information already lost before a value reaches this viewer.

Seven pure viewer tests passed. The regression covers Text, JsonText and UTF-8 Bytes with duplicate keys, large decimal text, exponent spelling, escapes, Unicode and whitespace. Existing tests cover typed JSON, NULL, binary/UTF-8 bytes, long hex dumps and display truncation.

The existing installed scenario expected pretty-printing of a SQLite text literal. It is updated for the reported behavior: display that stored text verbatim, including duplicate keys and numeric spelling. Historical evidence retains its old scenario name and result.

Full preflight passed: 2,208 unit tests, 291 sandbox tests and one Redis check. App Clippy passed. The rebuilt installed application passed `view_value_preserves_duplicate_json_keys_and_text_formatting` under isolated Xvfb, asserting the entire displayed buffer equals the SQLite text literal. The first installed attempt did not open the dialog through the inherited pointer/menu-position sequence. The final scenario uses the existing result-column keyboard navigation before opening the viewer. All 179 isolated GTK widget tests passed, including the full-value viewer test. Forgejo acceptance is deferred during infrastructure migration. App-services generated artifacts were removed from debug and ci profiles before broad validation to avoid stale binaries from another worktree.

Release-Note: Fixed: The value viewer preserves JSON-looking text without dropping duplicate keys or reformatting numbers.
