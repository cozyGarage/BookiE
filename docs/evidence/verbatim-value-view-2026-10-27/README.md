# Verbatim text value viewing, 2026-10-27

Issue #1744, lane UX. Source base: `887678414`. This change affects the read-only value viewer, not decoders, bindings or stored values.

The viewer previously parsed text beginning with `{` or `[` into a JSON object and pretty-printed it. A regression failed on the original code: `{"a":1,"a":2}` lost its first key. Parsing could also alter large numbers, exponent spelling, escapes and whitespace.

The text path now uses the existing full-value text renderer directly. Typed JSON still uses its existing pretty display, and binary values retain their hex display. The display cap and complete-value copy callback are unchanged. No claim is made about restoring information already lost before a value reaches this viewer.

Seven pure viewer tests passed. The regression covers Text, JsonText and UTF-8 Bytes with duplicate keys, large decimal text, exponent spelling, escapes, Unicode and whitespace. Existing tests cover typed JSON, NULL, binary/UTF-8 bytes, long hex dumps and display truncation.

The existing installed scenario expected pretty-printing of a SQLite text literal. It is updated for the reported behavior: display that stored text verbatim, including duplicate keys and numeric spelling. Historical evidence retains its old scenario name and result.

Full preflight passed: 2,208 unit tests, 291 sandbox tests and one Redis check. App Clippy passed. The full local CI tier later passed 3,012 workspace unit tests, 291 sandbox tests and one Redis check. The rebuilt installed application passed `view_value_preserves_duplicate_json_keys_and_text_formatting` under isolated Xvfb, asserting the entire displayed buffer equals the SQLite text literal. The first installed attempt did not open the dialog through the inherited pointer/menu-position sequence. The final scenario uses the existing result-column keyboard navigation before opening the viewer. All 179 isolated GTK widget tests passed, including the full-value viewer test. Forgejo acceptance is deferred during infrastructure migration. App-services generated artifacts were removed from debug and ci profiles before broad validation to avoid stale binaries from another worktree.

Release-Note: Fixed: The value viewer preserves JSON-looking text without dropping duplicate keys or reformatting numbers.

## Bounded preview allocation

Follow-up baseline: `9e415fdf5`. Even after removing JSON parsing, the viewer cloned the entire text value before applying its 1 MiB display cap. The new process-isolated regression failed on that baseline: a 64 MiB Text fixture added 69,968 KiB to peak RSS. The fixture remains alive during measurement. Each value kind runs in a fresh test process, using `/proc/self/status` VmHWM before and after rendering; the allowed increase is below 16 MiB.

The cap now accepts borrowed text for Text, JsonText and valid UTF-8 bytes. Only the bounded display prefix is copied. Binary hex output, typed JSON rendering, full copy behavior and truncation notices retain their existing behavior.

After the change, measured additional peaks were 4,444 KiB for Text, 4,360 KiB for JsonText and 4,636 KiB for UTF-8 Bytes. All eight pure viewer tests passed. These are synthetic debug-test process measurements, not whole-application or database throughput claims. Typed JSON serialization still produces its own formatted string before display capping.

The allocation follow-up passed full preflight (2,208 unit, 291 sandbox and one Redis check) and app Clippy. GTK validation is incomplete: the widget runner fails at GTK initialization in the workspace-overview test, and the installed run cannot access its X display before scenario interaction. A sequential widget rerun also failed at initialization. The failure reproduces in a minimal Rust GTK program and plain C `gtk_init_check()` under Xvfb; PyGObject reports success on that display. PyGObject marks GTK initialized during module import despite having no default display, so its `init_check()` result was not a valid comparison. GTK initialization still fails for both plain C and Rust under the local Xvfb fixture; the display failure remains unresolved. The earlier revision passed 179 widgets and the installed value scenario; those results do not prove the allocation follow-up. No acceptance pass is claimed. The shell hit a temporary-file quota before the first edit attempt; that attempt changed no source and selected zero tests, so it is excluded from validation. Subsequent builds use an owned temporary directory on the project disk. Unrelated temporary checkouts were left untouched.
